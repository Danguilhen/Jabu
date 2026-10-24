//! XMPP SFU conferences for space voice channels.
//!
//! This is the signalling layer described by ADR-014. A voice channel is a
//! normal MUC room; the SFU joins the room, advertises COIN or the `ProtoXEP`
//! audio/video-conference feature, and the client calls it with Jingle.
//!
//! Two dialects are supported:
//!
//! * **Galener** (Movim): one Jingle session per participant, COIN
//!   `conference-info` presence, `content-add`/`content-remove` carrying
//!   `<jingle-participant xmlns="{https://movim.eu}jingle_participant">` and
//!   XEP-0507 `<category name="speaker|slides">`.
//! * **`ProtoXEP`** *Jingle Audio/Video Conferences*: one session per
//!   participant and an `<origin xmlns="urn:xmpp:jingle:av-conferences:0"
//!   entity="xmpp:…"/>` identifying the peer.
//!
//! The module only deals with stanzas. SDP ↔ Jingle conversion and the
//! `webrtcbin` receive contents live in the media crate.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use jid::Jid;
use minidom::Element;
use parking_lot::RwLock;
use uuid::Uuid;
use xmpp_parsers::ns;

use crate::{Result, XmppError};

pub const NS_JINGLE: &str = "urn:xmpp:jingle:1";
pub const NS_JINGLE_MESSAGE: &str = "urn:xmpp:jingle-message:0";
pub const NS_JINGLE_RTP: &str = "urn:xmpp:jingle:apps:rtp:1";
pub const NS_JINGLE_ICE_UDP: &str = "urn:xmpp:jingle:transports:ice-udp:1";
pub const NS_JINGLE_GROUPING: &str = "urn:xmpp:jingle:apps:grouping:0";
pub const NS_JINGLE_CATEGORY: &str = "urn:xmpp:jingle:apps:category:0";
pub const NS_JINGLE_RTP_INFO: &str = "urn:xmpp:jingle:apps:rtp:info:1";
pub const NS_JINGLE_PARTICIPANT: &str = "{https://movim.eu}jingle_participant";
pub const NS_AV_CONFERENCES: &str = "urn:xmpp:jingle:av-conferences:0";
pub const NS_COIN: &str = "urn:xmpp:coin:1";
pub const NS_CONFERENCE_INFO: &str = "urn:ietf:params:xml:ns:conference-info";
pub const NS_CONFERENCE_STARTED_AT: &str = "{https://movim.eu}conference_started_at";
pub const NS_DISCO_INFO: &str = "http://jabber.org/protocol/disco#info";
pub const NS_MUC_USER: &str = "http://jabber.org/protocol/muc#user";
pub const NS_CAPS: &str = "urn:xmpp:caps";

/// Which SFU dialect a room uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConferenceDialect {
    /// Movim's Galener component (COIN, `jingle-participant`, categories).
    Galener,
    /// Standard Jingle Audio/Video Conferences (`<origin>`).
    ProtoXep,
}

impl ConferenceDialect {
    /// Pick the dialect from a disco#info feature list.
    #[must_use]
    pub fn from_features(features: &[String]) -> Option<Self> {
        if features.iter().any(|feature| feature == NS_COIN) {
            Some(Self::Galener)
        } else if features.iter().any(|feature| feature == NS_AV_CONFERENCES) {
            Some(Self::ProtoXep)
        } else {
            None
        }
    }
}

/// One participant advertised by COIN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConferenceUser {
    pub entity: Jid,
    pub state: String,
}

/// COIN `conference-info` carried in the SFU occupant's presence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConferenceInfo {
    pub entity: Option<Jid>,
    pub state: String,
    pub version: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub users: Vec<ConferenceUser>,
}

impl ConferenceInfo {
    /// Parse a `<conference-info xmlns='urn:ietf:params:xml:ns:conference-info'>`.
    #[must_use]
    pub fn parse(element: &Element) -> Option<Self> {
        if element.ns() != NS_CONFERENCE_INFO {
            return None;
        }
        let entity = element
            .attr("entity")
            .and_then(|value| value.strip_prefix("xmpp:").or(Some(value)))
            .and_then(|value| value.parse::<Jid>().ok());
        let started_at = element
            .get_child("conference-started-at", NS_CONFERENCE_STARTED_AT)
            .and_then(|child| child.attr("started-at"))
            .and_then(|value| value.parse::<DateTime<Utc>>().ok());
        let mut users = Vec::new();
        if let Some(list) = element.get_child("users", NS_CONFERENCE_INFO) {
            for user in list.children().filter(|child| child.name() == "user") {
                let Some(entity) = user
                    .attr("entity")
                    .and_then(|value| value.strip_prefix("xmpp:").or(Some(value)))
                    .and_then(|value| value.parse::<Jid>().ok())
                else {
                    continue;
                };
                users.push(ConferenceUser {
                    entity,
                    state: user.attr("state").unwrap_or("full").to_string(),
                });
            }
        }

        Some(Self {
            entity,
            state: element.attr("state").unwrap_or("full").to_string(),
            version: element.attr("version").map(str::to_string),
            started_at,
            users,
        })
    }
}

/// The SFU as discovered from a MUC occupant presence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfuDiscovery {
    pub room: Jid,
    /// The Jingle target: Galener's hashed virtual user or the `ProtoXEP` room.
    pub jid: Jid,
    pub nick: Option<String>,
    pub dialect: ConferenceDialect,
    pub info: Option<ConferenceInfo>,
}

/// Extract the SFU from a room presence.
///
/// Galener's virtual user is visible as `<item jid='<hash>@sfu.domain'>` and its
/// presence carries COIN `conference-info`. Normal participants are ignored.
#[must_use]
pub fn discover_sfu(room: &Jid, presence: &Element) -> Option<SfuDiscovery> {
    let from = presence.attr("from")?.parse::<Jid>().ok()?;
    if from.to_bare() != room.to_bare() {
        return None;
    }

    let info = presence
        .get_child("conference-info", NS_CONFERENCE_INFO)
        .and_then(ConferenceInfo::parse);

    let item_jid = presence
        .get_child("x", NS_MUC_USER)
        .and_then(|x| x.get_child("item", NS_MUC_USER))
        .and_then(|item| item.attr("jid"))
        .and_then(|value| value.parse::<Jid>().ok());

    let jid = item_jid.or_else(|| info.as_ref().and_then(|info| info.entity.clone()))?;
    let dialect = if info.is_some() {
        ConferenceDialect::Galener
    } else {
        ConferenceDialect::ProtoXep
    };
    // Without COIN, only a bare component JID (no localpart) is an SFU. A
    // normal participant must not be mistaken for the conference service.
    if info.is_none() && jid.node().is_some() {
        return None;
    }

    Some(SfuDiscovery {
        room: room.to_bare().into(),
        jid,
        nick: from.resource().map(ToString::to_string),
        dialect,
        info,
    })
}

/// XEP-0507 content categories used by the conference UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConferenceCategory {
    Speaker,
    Slides,
    Other(String),
}

impl ConferenceCategory {
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "speaker" => Self::Speaker,
            "slides" => Self::Slides,
            other => Self::Other(other.to_string()),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Speaker => "speaker",
            Self::Slides => "slides",
            Self::Other(other) => other,
        }
    }
}

/// One ICE candidate from `transport-info` or a transport element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JingleCandidate {
    pub foundation: String,
    pub component: u32,
    pub protocol: String,
    pub priority: u32,
    pub ip: String,
    pub port: u16,
    pub kind: String,
    pub rel_addr: Option<String>,
    pub rel_port: Option<u16>,
}

impl JingleCandidate {
    fn parse(element: &Element) -> Option<Self> {
        Some(Self {
            foundation: element.attr("foundation")?.to_string(),
            component: element.attr("component")?.parse().ok()?,
            protocol: element.attr("protocol").unwrap_or("udp").to_string(),
            priority: element.attr("priority")?.parse().ok()?,
            ip: element.attr("ip")?.to_string(),
            port: element.attr("port")?.parse().ok()?,
            kind: element.attr("type").unwrap_or("host").to_string(),
            rel_addr: element.attr("rel-addr").map(str::to_string),
            rel_port: element
                .attr("rel-port")
                .and_then(|value| value.parse().ok()),
        })
    }
}

/// A remote audio/video/screen content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConferenceContent {
    pub name: String,
    pub creator: String,
    pub senders: String,
    pub media: String,
    pub category: ConferenceCategory,
    pub participant: Option<Jid>,
    pub ssrc: Option<u32>,
    pub candidates: Vec<JingleCandidate>,
}

/// A parsed Jingle action relevant to a conference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConferenceJingle {
    pub action: String,
    pub sid: String,
    pub initiator: Option<Jid>,
    pub responder: Option<Jid>,
    /// Galener's non-standard participant element.
    pub participant: Option<Jid>,
    /// `ProtoXEP` `<origin entity='xmpp:…'/>`.
    pub origin: Option<Jid>,
    pub contents: Vec<ConferenceContent>,
    pub candidates: Vec<JingleCandidate>,
    pub mute: Option<(Option<String>, bool)>,
}

impl ConferenceJingle {
    /// Parse the `<jingle>` payload of an IQ.
    ///
    /// # Errors
    ///
    /// Returns an error when the element is not Jingle or lacks `action`/`sid`.
    pub fn parse(jingle: &Element) -> Result<Self> {
        if jingle.ns() != NS_JINGLE {
            return Err(XmppError::ProtocolError("not a Jingle element".into()));
        }
        let action = jingle
            .attr("action")
            .ok_or_else(|| XmppError::ProtocolError("Jingle without action".into()))?
            .to_string();
        let sid = jingle
            .attr("sid")
            .ok_or_else(|| XmppError::ProtocolError("Jingle without sid".into()))?
            .to_string();
        let initiator = jingle
            .attr("initiator")
            .and_then(|value| value.parse::<Jid>().ok());
        let responder = jingle
            .attr("responder")
            .and_then(|value| value.parse::<Jid>().ok());
        let participant = jingle
            .get_child("jingle-participant", NS_JINGLE_PARTICIPANT)
            .and_then(|element| element.attr("participant"))
            .and_then(|value| value.parse::<Jid>().ok());
        let origin = jingle
            .get_child("origin", NS_AV_CONFERENCES)
            .and_then(|element| element.attr("entity"))
            .and_then(|value| value.strip_prefix("xmpp:").or(Some(value)))
            .and_then(|value| value.parse::<Jid>().ok());
        // In ProtoXEP the content origin is the peer that owns the stream; in
        // Galener the same role is carried by `jingle-participant`.
        let participant = participant.or_else(|| origin.clone());

        let mut contents = Vec::new();
        for content in jingle.children().filter(|child| child.name() == "content") {
            let description = content.get_child("description", NS_JINGLE_RTP);
            let media = description
                .and_then(|description| description.attr("media"))
                .unwrap_or("audio")
                .to_string();
            let category = description
                .and_then(|description| description.get_child("category", NS_JINGLE_CATEGORY))
                .and_then(|category| category.attr("name"))
                .map_or(ConferenceCategory::Speaker, ConferenceCategory::from_name);
            let ssrc = description
                .and_then(|description| {
                    description.get_child("source", "urn:xmpp:jingle:apps:rtp:ssma:0")
                })
                .and_then(|source| source.attr("ssrc"))
                .and_then(|value| value.parse().ok());

            let transport = content.get_child("transport", NS_JINGLE_ICE_UDP);

            contents.push(ConferenceContent {
                name: content.attr("name").unwrap_or("0").to_string(),
                creator: content.attr("creator").unwrap_or("initiator").to_string(),
                senders: content.attr("senders").unwrap_or("both").to_string(),
                media,
                category,
                participant: participant.clone(),
                ssrc,
                candidates: transport
                    .map(|transport| {
                        transport
                            .children()
                            .filter(|child| child.name() == "candidate")
                            .filter_map(JingleCandidate::parse)
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        }

        let candidates = contents
            .iter()
            .flat_map(|content| content.candidates.clone())
            .collect();

        let mute = jingle
            .get_child("mute", NS_JINGLE_RTP_INFO)
            .map(|element| (element.attr("name").map(str::to_string), true))
            .or_else(|| {
                jingle
                    .get_child("unmute", NS_JINGLE_RTP_INFO)
                    .map(|element| (element.attr("name").map(str::to_string), false))
            });

        Ok(Self {
            action,
            sid,
            initiator,
            responder,
            participant,
            origin,
            contents,
            candidates,
            mute,
        })
    }
}

/// Conference lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConferenceState {
    Proposing,
    Connecting,
    Active,
    Ended,
}

/// One active conference, keyed by room.
#[derive(Debug, Clone)]
pub struct ConferenceSession {
    pub room: Jid,
    pub sfu: Jid,
    pub dialect: ConferenceDialect,
    pub sid: String,
    pub state: ConferenceState,
    pub sfu_nick: Option<String>,
    pub participants: HashMap<String, Jid>,
    pub contents: HashMap<String, ConferenceContent>,
    /// Per-participant mute state, keyed by bare JID.
    pub muted: HashMap<String, bool>,
    pub created_at: DateTime<Utc>,
}

impl ConferenceSession {
    #[must_use]
    pub fn participant(&self, jid: &Jid) -> bool {
        self.participants.contains_key(&jid.to_bare().to_string())
    }

    /// Contents belonging to a participant, in stream order.
    #[must_use]
    pub fn contents_for(&self, jid: &Jid) -> Vec<&ConferenceContent> {
        self.contents
            .values()
            .filter(|content| {
                content
                    .participant
                    .as_ref()
                    .is_some_and(|participant| participant.to_bare() == jid.to_bare())
            })
            .collect()
    }
}

/// Events emitted by the conference signalling layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConferenceEvent {
    Proceed {
        sid: String,
    },
    Accepted {
        sid: String,
    },
    ContentAdded {
        sid: String,
        content: ConferenceContent,
    },
    ContentRemoved {
        sid: String,
        names: Vec<String>,
        participant: Option<Jid>,
    },
    Candidate {
        sid: String,
        candidate: JingleCandidate,
    },
    Muted {
        sid: String,
        participant: Option<Jid>,
        content: Option<String>,
        muted: bool,
    },
    Ended {
        sid: String,
        reason: String,
    },
}

/// The result of feeding one stanza to the manager.
#[derive(Debug, Default)]
pub struct ConferenceUpdate {
    /// IQ result to return, if the stanza was an IQ.
    pub reply: Option<Element>,
    pub events: Vec<ConferenceEvent>,
    pub handled: bool,
}

/// Conference signalling state for every room of one account.
/// The result of inspecting a room presence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoicePresence {
    Discovered(SfuDiscovery),
    Ended { room: Jid },
}

#[derive(Default)]
pub struct ConferenceManager {
    /// Keyed by Jingle session id: Galener opens a second session for screen
    /// sharing, so one room can have more than one session at a time.
    sessions: RwLock<HashMap<String, ConferenceSession>>,
    /// `room|sfu` pairs already reported, so a presence storm does not emit
    /// duplicate discovery notifications.
    known: RwLock<HashSet<String>>,
    /// The SFU discovered in each room.
    discoveries: RwLock<HashMap<String, SfuDiscovery>>,
    /// Playback gain per `room|participant`.
    volumes: RwLock<HashMap<String, f64>>,
}

impl ConferenceManager {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The primary active session for a room.
    #[must_use]
    pub fn session(&self, room: &Jid) -> Option<ConferenceSession> {
        self.sessions
            .read()
            .values()
            .filter(|session| {
                session.room.to_bare() == room.to_bare() && session.state != ConferenceState::Ended
            })
            .min_by_key(|session| session.created_at)
            .cloned()
    }

    /// A specific Jingle session.
    #[must_use]
    pub fn session_by_sid(&self, sid: &str) -> Option<ConferenceSession> {
        self.sessions.read().get(sid).cloned()
    }

    /// Playback gain for one participant (1.0 is 100%).
    #[must_use]
    pub fn participant_volume(&self, room: &Jid, participant: &Jid) -> f64 {
        self.volumes
            .read()
            .get(&format!("{}|{}", room.to_bare(), participant.to_bare()))
            .copied()
            .unwrap_or(1.0)
    }

    /// Set the playback gain for one participant.
    pub fn set_participant_volume(&self, room: &Jid, participant: &Jid, volume: f64) {
        self.volumes.write().insert(
            format!("{}|{}", room.to_bare(), participant.to_bare()),
            volume.clamp(0.0, 2.0),
        );
    }

    /// The SFU discovered in a room, if any.
    #[must_use]
    pub fn discovery(&self, room: &Jid) -> Option<SfuDiscovery> {
        self.discoveries
            .read()
            .get(&room.to_bare().to_string())
            .cloned()
    }

    /// Whether `sid` belongs to a conference session.
    #[must_use]
    pub fn owns_sid(&self, sid: &str) -> bool {
        self.sessions.read().contains_key(sid)
    }

    /// Whether `jid` is the SFU of any active conference.
    #[must_use]
    pub fn has_sfu(&self, jid: &Jid) -> bool {
        self.sessions
            .read()
            .values()
            .any(|session| session.sfu.to_bare() == jid.to_bare())
    }

    /// Inspect a MUC presence and update conference discovery state.
    pub fn observe_presence(&self, presence: &Element) -> Option<VoicePresence> {
        let from = presence.attr("from")?.parse::<Jid>().ok()?;
        let room = from.to_bare();

        if presence.attr("type") == Some("unavailable") {
            let nick = from.resource().map(ToString::to_string);
            let mut sessions = self.sessions.write();
            let leaving = sessions
                .values()
                .any(|session| session.room.to_bare() == room && session.sfu_nick == nick);
            if !leaving {
                return None;
            }
            let ids: Vec<String> = sessions
                .iter()
                .filter(|(_, session)| session.room.to_bare() == room)
                .map(|(sid, _)| sid.clone())
                .collect();
            for sid in ids {
                sessions.remove(&sid);
            }
            drop(sessions);
            self.discoveries.write().remove(&room.to_string());
            let prefix = format!("{room}|");
            self.known.write().retain(|key| !key.starts_with(&prefix));
            self.volumes
                .write()
                .retain(|key, _| !key.starts_with(&prefix));
            return Some(VoicePresence::Ended { room: room.into() });
        }

        let discovery = discover_sfu(&room, presence)?;

        if let Some(info) = &discovery.info {
            let mut sessions = self.sessions.write();
            for session in sessions
                .values_mut()
                .filter(|session| session.room.to_bare() == room)
            {
                for user in &info.users {
                    session.participants.insert(
                        user.entity.to_bare().to_string(),
                        user.entity.to_bare().into(),
                    );
                }
            }
        }

        self.discoveries
            .write()
            .insert(room.to_string(), discovery.clone());

        let key = format!("{room}|{}", discovery.jid);
        if !self.known.write().insert(key) {
            return None;
        }

        Some(VoicePresence::Discovered(discovery))
    }

    #[must_use]
    pub fn sessions(&self) -> Vec<ConferenceSession> {
        self.sessions.read().values().cloned().collect()
    }

    /// Register a newly discovered SFU and generate a fresh session id.
    pub fn begin(&self, discovery: &SfuDiscovery) -> ConferenceSession {
        self.begin_with_sid(discovery, &Uuid::new_v4().to_string())
    }

    /// Register a session with a caller-chosen id (used by tests and restored
    /// sessions).
    pub fn begin_with_sid(&self, discovery: &SfuDiscovery, sid: &str) -> ConferenceSession {
        let mut session = ConferenceSession {
            room: discovery.room.to_bare().into(),
            sfu: discovery.jid.clone(),
            dialect: discovery.dialect,
            sid: sid.to_string(),
            state: ConferenceState::Proposing,
            sfu_nick: discovery.nick.clone(),
            participants: HashMap::new(),
            contents: HashMap::new(),
            muted: HashMap::new(),
            created_at: Utc::now(),
        };
        if let Some(info) = &discovery.info {
            for user in &info.users {
                session.participants.insert(
                    user.entity.to_bare().to_string(),
                    user.entity.to_bare().into(),
                );
            }
        }
        self.sessions
            .write()
            .insert(sid.to_string(), session.clone());
        session
    }

    /// Remove every session of a room when the user leaves it.
    pub fn remove(&self, room: &Jid) -> Option<ConferenceSession> {
        let mut sessions = self.sessions.write();
        let ids: Vec<String> = sessions
            .iter()
            .filter(|(_, session)| session.room.to_bare() == room.to_bare())
            .map(|(sid, _)| sid.clone())
            .collect();
        let first = ids.first().and_then(|sid| sessions.get(sid)).cloned();
        for sid in ids {
            sessions.remove(&sid);
        }
        first
    }

    /// Feed a Jingle IQ. Returns the IQ result and derived events.
    ///
    /// # Errors
    ///
    /// Returns an error when the Jingle payload is malformed.
    #[allow(clippy::too_many_lines)]
    pub fn handle_jingle(&self, elem: &Element) -> Result<ConferenceUpdate> {
        let Some(jingle) = elem.get_child("jingle", NS_JINGLE) else {
            return Ok(ConferenceUpdate::default());
        };
        let parsed = ConferenceJingle::parse(jingle)?;
        let mut update = ConferenceUpdate {
            reply: Some(iq_result(elem)),
            events: Vec::new(),
            handled: true,
        };
        let mut sessions = self.sessions.write();
        let Some(session) = sessions.get_mut(&parsed.sid) else {
            drop(sessions);
            return Ok(update);
        };

        match parsed.action.as_str() {
            "session-accept" => {
                session.state = ConferenceState::Active;
                for content in &parsed.contents {
                    session
                        .contents
                        .insert(content.name.clone(), content.clone());
                    if let Some(participant) = &content.participant {
                        session.participants.insert(
                            participant.to_bare().to_string(),
                            participant.to_bare().into(),
                        );
                    }
                }
                update.events.push(ConferenceEvent::Accepted {
                    sid: parsed.sid.clone(),
                });
            }
            "content-add" => {
                for content in &parsed.contents {
                    session
                        .contents
                        .insert(content.name.clone(), content.clone());
                    if let Some(participant) = content.participant.clone() {
                        session.participants.insert(
                            participant.to_bare().to_string(),
                            participant.to_bare().into(),
                        );
                        update.events.push(ConferenceEvent::ContentAdded {
                            sid: parsed.sid.clone(),
                            content: content.clone(),
                        });
                    }
                }
            }
            "content-remove" => {
                let names: Vec<String> = parsed
                    .contents
                    .iter()
                    .map(|content| content.name.clone())
                    .collect();
                for name in &names {
                    session.contents.remove(name);
                }
                update.events.push(ConferenceEvent::ContentRemoved {
                    sid: parsed.sid.clone(),
                    names,
                    participant: parsed.participant.clone(),
                });
            }
            "transport-info" => {
                for candidate in &parsed.candidates {
                    update.events.push(ConferenceEvent::Candidate {
                        sid: parsed.sid.clone(),
                        candidate: candidate.clone(),
                    });
                }
                for content in &parsed.contents {
                    for candidate in &content.candidates {
                        update.events.push(ConferenceEvent::Candidate {
                            sid: parsed.sid.clone(),
                            candidate: candidate.clone(),
                        });
                    }
                }
            }
            "session-info" => {
                if let Some((content, muted)) = &parsed.mute {
                    if let Some(participant) = &parsed.participant {
                        session
                            .muted
                            .insert(participant.to_bare().to_string(), *muted);
                    }
                    update.events.push(ConferenceEvent::Muted {
                        sid: parsed.sid.clone(),
                        participant: parsed.participant.clone(),
                        content: content.clone(),
                        muted: *muted,
                    });
                }
            }
            "session-terminate" => {
                session.state = ConferenceState::Ended;
                let reason = elem
                    .get_child("jingle", NS_JINGLE)
                    .and_then(|jingle| jingle.get_child("reason", NS_JINGLE))
                    .and_then(|reason| reason.children().next())
                    .map_or_else(|| "ended".to_string(), |child| child.name().to_string());
                update.events.push(ConferenceEvent::Ended {
                    sid: parsed.sid.clone(),
                    reason,
                });
            }
            _ => {}
        }

        Ok(update)
    }

    /// Feed a Jingle Message Initiation message (`proceed`, `reject`, ...).
    ///
    /// # Errors
    ///
    /// Returns an error when the Jingle payload is malformed.
    #[allow(clippy::significant_drop_tightening)]
    pub fn handle_message(&self, elem: &Element) -> Result<ConferenceUpdate> {
        let Some(child) = elem
            .children()
            .find(|child| child.ns() == NS_JINGLE_MESSAGE)
        else {
            return Ok(ConferenceUpdate::default());
        };
        let Some(id) = child.attr("id") else {
            return Ok(ConferenceUpdate::default());
        };
        let mut update = ConferenceUpdate {
            reply: None,
            events: Vec::new(),
            handled: true,
        };
        let mut sessions = self.sessions.write();
        if let Some(session) = sessions.get_mut(id) {
            match child.name() {
                "proceed" => {
                    session.state = ConferenceState::Connecting;
                    update.events.push(ConferenceEvent::Proceed {
                        sid: id.to_string(),
                    });
                }
                "reject" | "retract" => {
                    session.state = ConferenceState::Ended;
                    update.events.push(ConferenceEvent::Ended {
                        sid: id.to_string(),
                        reason: child.name().to_string(),
                    });
                }
                "finish" => {
                    session.state = ConferenceState::Ended;
                    update.events.push(ConferenceEvent::Ended {
                        sid: id.to_string(),
                        reason: "finished".to_string(),
                    });
                }
                _ => {}
            }
        }
        Ok(update)
    }
}

/// Build the `propose` message that opens a Galener/PotoXEP call.
#[must_use]
pub fn propose_message(to: &Jid, sid: &str, with_video: bool) -> Element {
    let mut propose = Element::builder("propose", NS_JINGLE_MESSAGE).attr("id", sid.to_string());
    propose = propose.append(Element::builder("description", NS_JINGLE_RTP).attr("media", "audio"));
    if with_video {
        propose =
            propose.append(Element::builder("description", NS_JINGLE_RTP).attr("media", "video"));
    }
    Element::builder("message", ns::JABBER_CLIENT)
        .attr("type", "chat")
        .attr("to", to.to_string())
        .append(propose)
        .build()
}

/// Wrap an already built Jingle offer in `session-initiate`.
#[must_use]
pub fn session_initiate_iq(to: &Jid, sid: &str, initiator: &Jid, payload: Element) -> Element {
    let jingle = Element::builder("jingle", NS_JINGLE)
        .attr("action", "session-initiate")
        .attr("sid", sid.to_string())
        .attr("initiator", initiator.to_string())
        .append(payload)
        .build();
    Element::builder("iq", ns::JABBER_CLIENT)
        .attr("type", "set")
        .attr("id", Uuid::new_v4().to_string())
        .attr("to", to.to_string())
        .append(jingle)
        .build()
}

/// Send a media-produced `<jingle>` offer as the conference session-initiate.
///
/// The offer comes from the media backend; this sets the conference sid and
/// marks every content send-only, which is what an SFU participant does.
#[must_use]
pub fn session_initiate_with_offer(
    to: &Jid,
    sid: &str,
    initiator: &Jid,
    mut offer: Element,
    dialect: ConferenceDialect,
) -> Element {
    offer.set_attr("action", "session-initiate");
    offer.set_attr("sid", sid.to_string());
    offer.set_attr("initiator", initiator.to_string());
    for content in offer
        .children_mut()
        .filter(|child| child.name() == "content")
    {
        if content.attr("senders").is_none() {
            content.set_attr("senders", "initiator");
        }
    }
    if dialect == ConferenceDialect::ProtoXep {
        offer.append_child(
            Element::builder("origin", NS_AV_CONFERENCES)
                .attr("entity", format!("xmpp:{initiator}"))
                .build(),
        );
    }
    Element::builder("iq", ns::JABBER_CLIENT)
        .attr("type", "set")
        .attr("id", Uuid::new_v4().to_string())
        .attr("to", to.to_string())
        .append(offer)
        .build()
}

/// Answer a remote `content-add` with a media-produced content element.
#[must_use]
pub fn content_accept_iq(
    to: &Jid,
    sid: &str,
    responder: &Jid,
    participant: &Jid,
    mut content: Element,
    dialect: ConferenceDialect,
) -> Element {
    content.set_attr("senders", "responder");
    content_iq(
        "content-accept",
        to,
        sid,
        responder,
        Some(participant),
        content,
        dialect,
    )
}

/// Reject a remote content.
#[must_use]
pub fn content_reject_iq(to: &Jid, sid: &str, responder: &Jid, names: &[String]) -> Element {
    let mut jingle = Element::builder("jingle", NS_JINGLE)
        .attr("action", "content-reject")
        .attr("sid", sid.to_string())
        .attr("responder", responder.to_string());
    for name in names {
        jingle = jingle.append(
            Element::builder("content", NS_JINGLE)
                .attr("creator", "initiator")
                .attr("name", name.clone()),
        );
    }
    Element::builder("iq", ns::JABBER_CLIENT)
        .attr("type", "set")
        .attr("id", Uuid::new_v4().to_string())
        .attr("to", to.to_string())
        .append(jingle)
        .build()
}

fn content_iq(
    action: &str,
    to: &Jid,
    sid: &str,
    responder: &Jid,
    participant: Option<&Jid>,
    content: Element,
    dialect: ConferenceDialect,
) -> Element {
    let mut jingle = Element::builder("jingle", NS_JINGLE)
        .attr("action", action)
        .attr("sid", sid.to_string())
        .attr("responder", responder.to_string())
        .append(content);
    if let Some(participant) = participant {
        if dialect == ConferenceDialect::ProtoXep {
            jingle = jingle.append(
                Element::builder("origin", NS_AV_CONFERENCES)
                    .attr("entity", format!("xmpp:{participant}")),
            );
        } else {
            jingle = jingle.append(
                Element::builder("jingle-participant", NS_JINGLE_PARTICIPANT)
                    .attr("participant", participant.to_string()),
            );
        }
    }
    Element::builder("iq", ns::JABBER_CLIENT)
        .attr("type", "set")
        .attr("id", Uuid::new_v4().to_string())
        .attr("to", to.to_string())
        .append(jingle)
        .build()
}

/// Forward a mute/unmute to the SFU (Galener relays it to the other members).
#[must_use]
pub fn session_info_iq(to: &Jid, sid: &str, content: Option<&str>, muted: bool) -> Element {
    let mut info = Element::builder(if muted { "mute" } else { "unmute" }, NS_JINGLE_RTP_INFO);
    if let Some(content) = content {
        info = info.attr("name", content.to_string());
    }
    let jingle = Element::builder("jingle", NS_JINGLE)
        .attr("action", "session-info")
        .attr("sid", sid.to_string())
        .append(info)
        .build();
    Element::builder("iq", ns::JABBER_CLIENT)
        .attr("type", "set")
        .attr("id", Uuid::new_v4().to_string())
        .attr("to", to.to_string())
        .append(jingle)
        .build()
}

/// Terminate the conference session.
#[must_use]
pub fn session_terminate_iq(to: &Jid, sid: &str, reason: &str) -> Element {
    let jingle = Element::builder("jingle", NS_JINGLE)
        .attr("action", "session-terminate")
        .attr("sid", sid.to_string())
        .append(Element::builder("reason", NS_JINGLE).append(Element::builder(reason, NS_JINGLE)))
        .build();
    Element::builder("iq", ns::JABBER_CLIENT)
        .attr("type", "set")
        .attr("id", Uuid::new_v4().to_string())
        .attr("to", to.to_string())
        .append(jingle)
        .build()
}

/// Standard IQ result for a Jingle request.
#[must_use]
pub fn iq_result(request: &Element) -> Element {
    let mut result = Element::builder("iq", ns::JABBER_CLIENT).attr("type", "result");
    if let Some(id) = request.attr("id") {
        result = result.attr("id", id.to_string());
    }
    if let Some(from) = request.attr("from") {
        result = result.attr("to", from.to_string());
    }
    result.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISCO: &str = include_str!("../tests/golden/galener/disco_info.xml");
    const COIN: &str = include_str!("../tests/golden/galener/coin_presence.xml");
    const PROCEED: &str = include_str!("../tests/golden/galener/proceed.xml");
    const ACCEPT: &str = include_str!("../tests/golden/galener/session_accept.xml");
    const TRANSPORT: &str = include_str!("../tests/golden/galener/transport_info.xml");
    const ADD_AUDIO: &str = include_str!("../tests/golden/galener/content_add_audio.xml");
    const ADD_SLIDES: &str = include_str!("../tests/golden/galener/content_add_slides.xml");
    const REMOVE_AUDIO: &str = include_str!("../tests/golden/galener/content_remove_audio.xml");
    const MUTE: &str = include_str!("../tests/golden/galener/session_info_mute_camera.xml");

    fn element(xml: &str) -> Element {
        xml.parse().expect("fixture parses")
    }

    #[test]
    fn coin_presence_finds_the_sfu() {
        let room: Jid = "voice@conference.localhost".parse().unwrap();
        let presence = element(COIN);
        let discovery = discover_sfu(&room, &presence).expect("SFU discovered");
        assert_eq!(discovery.dialect, ConferenceDialect::Galener);
        assert_eq!(
            discovery.jid.to_string(),
            "de2f62f2109432c4d0aabe000e0979a1afea1f60@sfu.example.org"
        );
        let info = discovery.info.expect("COIN info");
        assert_eq!(info.users.len(), 2);
        assert!(info.started_at.is_some());
    }

    #[test]
    fn disco_selects_the_dialect() {
        let disco = element(DISCO);
        let features: Vec<String> = disco
            .get_child("query", NS_DISCO_INFO)
            .expect("query")
            .children()
            .filter(|child| child.name() == "feature")
            .filter_map(|child| child.attr("var").map(str::to_string))
            .collect();
        assert_eq!(
            ConferenceDialect::from_features(&features),
            Some(ConferenceDialect::Galener)
        );
        assert_eq!(
            ConferenceDialect::from_features(&["urn:xmpp:jingle:av-conferences:0".into()]),
            Some(ConferenceDialect::ProtoXep)
        );
    }

    #[test]
    fn proceed_and_session_accept_parse() {
        let manager = ConferenceManager::new();
        let room: Jid = "voice@conference.localhost".parse().unwrap();
        let discovery = discover_sfu(&room, &element(COIN)).unwrap();
        let sid = "61bb024bad5548998cae554bc66a428b".to_string();
        manager.begin_with_sid(&discovery, &sid);

        let proceed = element(PROCEED);
        let mut proceed = proceed;
        proceed.set_attr("to", "alice@localhost/desktop");
        let update = manager.handle_message(&proceed).unwrap();
        assert_eq!(update.events, vec![ConferenceEvent::Proceed { sid }]);

        let accept = element(ACCEPT);
        let update = manager.handle_jingle(&accept).unwrap();
        assert!(update.reply.is_some());
        assert!(matches!(
            update.events.first(),
            Some(ConferenceEvent::Accepted { .. })
        ));
        assert_eq!(
            manager.session(&room).unwrap().state,
            ConferenceState::Active
        );
    }

    #[test]
    fn content_add_audio_and_slides() {
        let manager = ConferenceManager::new();
        let room: Jid = "voice@conference.localhost".parse().unwrap();
        let discovery = discover_sfu(&room, &element(COIN)).unwrap();
        manager.begin_with_sid(&discovery, "a1b2c3d4e5f60718293a4b5c6d7e8f90");

        let update = manager.handle_jingle(&element(ADD_AUDIO)).unwrap();
        let Some(ConferenceEvent::ContentAdded { content, .. }) = update.events.first() else {
            panic!("content-add event");
        };
        assert_eq!(content.category, ConferenceCategory::Speaker);
        assert_eq!(content.media, "audio");
        assert_eq!(
            content.participant.as_ref().unwrap().to_string(),
            "bob@localhost"
        );
        assert!(content.ssrc.is_some());

        manager.begin_with_sid(&discovery, "a1b2c3d4e5f60718293a4b5c6d7e8f90-slides");
        let update = manager.handle_jingle(&element(ADD_SLIDES)).unwrap();
        let Some(ConferenceEvent::ContentAdded { content, .. }) = update.events.first() else {
            panic!("content-add event");
        };
        assert_eq!(content.category, ConferenceCategory::Slides);
        assert_eq!(content.media, "video");
        assert_eq!(
            manager
                .session_by_sid("a1b2c3d4e5f60718293a4b5c6d7e8f90")
                .unwrap()
                .contents
                .len(),
            1
        );

        let update = manager.handle_jingle(&element(REMOVE_AUDIO)).unwrap();
        let Some(ConferenceEvent::ContentRemoved { names, .. }) = update.events.first() else {
            panic!("content-remove event");
        };
        assert_eq!(names, &["0".to_string()]);
    }

    #[test]
    fn transport_info_and_mute() {
        let manager = ConferenceManager::new();
        let room: Jid = "voice@conference.localhost".parse().unwrap();
        let discovery = discover_sfu(&room, &element(COIN)).unwrap();
        manager.begin_with_sid(&discovery, "61bb024bad5548998cae554bc66a428b");
        manager.begin_with_sid(&discovery, "a1b2c3d4e5f60718293a4b5c6d7e8f90");
        manager.handle_jingle(&element(ACCEPT)).unwrap();

        let update = manager.handle_jingle(&element(TRANSPORT)).unwrap();
        assert!(matches!(
            update.events.first(),
            Some(ConferenceEvent::Candidate { .. })
        ));

        let update = manager.handle_jingle(&element(MUTE)).unwrap();
        assert_eq!(
            update.events,
            vec![ConferenceEvent::Muted {
                sid: "a1b2c3d4e5f60718293a4b5c6d7e8f90".into(),
                participant: Some("bob@localhost".parse().unwrap()),
                content: Some("camera".into()),
                muted: true,
            }]
        );
        assert_eq!(
            manager
                .session_by_sid("a1b2c3d4e5f60718293a4b5c6d7e8f90")
                .unwrap()
                .muted
                .get("bob@localhost"),
            Some(&true)
        );

        let bob: Jid = "bob@localhost".parse().unwrap();
        manager.set_participant_volume(&room, &bob, 0.35);
        assert!((manager.participant_volume(&room, &bob) - 0.35).abs() < 0.001);
    }

    #[test]
    fn outgoing_stanzas_are_well_formed() {
        let to: Jid = "sfu@example.org".parse().unwrap();
        let own: Jid = "alice@localhost/desktop".parse().unwrap();

        let propose = propose_message(&to, "sid1", false);
        assert_eq!(propose.name(), "message");
        let child = propose.get_child("propose", NS_JINGLE_MESSAGE).unwrap();
        assert_eq!(child.attr("id"), Some("sid1"));
        assert!(child.get_child("description", NS_JINGLE_RTP).is_some());

        let content = Element::builder("content", NS_JINGLE)
            .attr("creator", "responder")
            .attr("name", "0")
            .build();
        let accept = content_accept_iq(
            &to,
            "sid1",
            &own,
            &"bob@localhost".parse().unwrap(),
            content,
            ConferenceDialect::Galener,
        );
        let jingle = accept.get_child("jingle", NS_JINGLE).unwrap();
        assert_eq!(jingle.attr("action"), Some("content-accept"));
        assert!(jingle
            .get_child("jingle-participant", NS_JINGLE_PARTICIPANT)
            .is_some());
        let content = jingle
            .children()
            .find(|child| child.name() == "content")
            .unwrap();
        assert_eq!(content.attr("senders"), Some("responder"));

        let mute = session_info_iq(&to, "sid1", Some("0"), true);
        let jingle = mute.get_child("jingle", NS_JINGLE).unwrap();
        assert!(jingle.get_child("mute", NS_JINGLE_RTP_INFO).is_some());

        let terminate = session_terminate_iq(&to, "sid1", "success");
        let jingle = terminate.get_child("jingle", NS_JINGLE).unwrap();
        assert_eq!(jingle.attr("action"), Some("session-terminate"));

        let offer: Element = "<jingle xmlns='urn:xmpp:jingle:1' sid='ignored'><content creator='initiator' name='0'><description xmlns='urn:xmpp:jingle:apps:rtp:1' media='audio'/></content></jingle>"
            .parse()
            .unwrap();
        let initiate =
            session_initiate_with_offer(&to, "conf1", &own, offer, ConferenceDialect::ProtoXep);
        let jingle = initiate.get_child("jingle", NS_JINGLE).unwrap();
        let origin = jingle.get_child("origin", NS_AV_CONFERENCES).unwrap();
        assert_eq!(origin.attr("entity"), Some("xmpp:alice@localhost/desktop"));
        assert_eq!(jingle.attr("action"), Some("session-initiate"));
        assert_eq!(jingle.attr("sid"), Some("conf1"));
        assert_eq!(jingle.attr("initiator"), Some("alice@localhost/desktop"));
        let content = jingle.get_child("content", NS_JINGLE).unwrap();
        assert_eq!(content.attr("senders"), Some("initiator"));
    }

    #[test]
    fn presence_discovery_is_deduplicated_and_ends() {
        let manager = ConferenceManager::new();
        let coin = element(COIN);
        let room: Jid = "voice@conference.localhost".parse().unwrap();

        let Some(VoicePresence::Discovered(discovery)) = manager.observe_presence(&coin) else {
            panic!("first presence discovers the SFU");
        };
        assert_eq!(
            discovery.jid.to_string(),
            "de2f62f2109432c4d0aabe000e0979a1afea1f60@sfu.example.org"
        );
        assert!(
            manager.observe_presence(&coin).is_none(),
            "duplicate is ignored"
        );

        manager.begin_with_sid(&discovery, "sid1");
        assert!(manager.has_sfu(&discovery.jid));
        assert!(manager.owns_sid("sid1"));

        let gone: Element = format!(
            "<presence xmlns='jabber:client' type='unavailable' from='{}/{}'><x xmlns='http://jabber.org/protocol/muc#user'><item affiliation='none' role='none' jid='{}'/></x></presence>",
            room, discovery.nick.as_deref().unwrap_or("sfu"), discovery.jid
        )
        .parse()
        .unwrap();
        assert_eq!(
            manager.observe_presence(&gone),
            Some(VoicePresence::Ended { room: room.clone() })
        );
        assert!(!manager.owns_sid("sid1"));
        assert!(manager.discovery(&room).is_none());
    }

    #[test]
    fn normal_participant_is_not_an_sfu() {
        let room: Jid = "voice@conference.localhost".parse().unwrap();
        let presence: Element = "<presence xmlns='jabber:client' from='voice@conference.localhost/alice'><x xmlns='http://jabber.org/protocol/muc#user'><item affiliation='member' role='participant' jid='alice@localhost/desktop'/></x></presence>"
            .parse()
            .unwrap();
        assert!(discover_sfu(&room, &presence).is_none());
    }

    #[test]
    fn session_terminate_hides_the_room_session() {
        let manager = ConferenceManager::new();
        let room: Jid = "voice@conference.localhost".parse().unwrap();
        let discovery = discover_sfu(&room, &element(COIN)).unwrap();
        let sid = "61bb024bad5548998cae554bc66a428b";
        manager.begin_with_sid(&discovery, sid);

        let to: Jid = "de2f62f2109432c4d0aabe000e0979a1afea1f60@sfu.example.org"
            .parse()
            .unwrap();
        let terminate = session_terminate_iq(&to, sid, "success");
        let update = manager.handle_jingle(&terminate).unwrap();
        assert!(matches!(
            update.events.first(),
            Some(ConferenceEvent::Ended { .. })
        ));
        assert!(manager.session(&room).is_none());
    }

    #[test]
    fn protoxep_content_uses_origin_as_participant() {
        let manager = ConferenceManager::new();
        let room: Jid = "voice@conference.example.org".parse().unwrap();
        let sfu: Jid = "room@conference.example.org".parse().unwrap();
        let discovery = SfuDiscovery {
            room,
            jid: sfu.clone(),
            nick: None,
            dialect: ConferenceDialect::ProtoXep,
            info: None,
        };
        let sid = "px1";
        manager.begin_with_sid(&discovery, sid);

        let add = format!(
            "<iq xmlns='jabber:client' type='set' id='add' from='{sfu}'><jingle xmlns='urn:xmpp:jingle:1' action='content-add' sid='{sid}'><origin xmlns='urn:xmpp:jingle:av-conferences:0' entity='xmpp:bob@example.org'/><content creator='initiator' name='audio' senders='initiator'><description xmlns='urn:xmpp:jingle:apps:rtp:1' media='audio'><category xmlns='urn:xmpp:jingle:apps:category:0' name='speaker'/></description></content></jingle></iq>"
        );
        let update = manager.handle_jingle(&element(&add)).unwrap();
        let Some(ConferenceEvent::ContentAdded { content, .. }) = update.events.first() else {
            panic!("content-add event");
        };
        assert_eq!(
            content.participant.as_ref().map(ToString::to_string),
            Some("bob@example.org".to_string())
        );

        let remove = format!(
            "<iq xmlns='jabber:client' type='set' id='remove' from='{sfu}'><jingle xmlns='urn:xmpp:jingle:1' action='content-remove' sid='{sid}'><origin xmlns='urn:xmpp:jingle:av-conferences:0' entity='xmpp:bob@example.org'/><content creator='initiator' name='audio'/></jingle></iq>"
        );
        let update = manager.handle_jingle(&element(&remove)).unwrap();
        let Some(ConferenceEvent::ContentRemoved {
            names, participant, ..
        }) = update.events.first()
        else {
            panic!("content-remove event");
        };
        assert_eq!(names, &["audio".to_string()]);
        assert_eq!(
            participant.as_ref().map(ToString::to_string),
            Some("bob@example.org".to_string())
        );
    }

    #[test]
    fn protoxep_content_accept_carries_origin() {
        let to: Jid = "room@conference.example.org".parse().unwrap();
        let own: Jid = "alice@example.org/desktop".parse().unwrap();
        let bob: Jid = "bob@example.org".parse().unwrap();
        let content = Element::builder("content", NS_JINGLE)
            .attr("creator", "initiator")
            .attr("name", "video")
            .build();
        let accept =
            content_accept_iq(&to, "px1", &own, &bob, content, ConferenceDialect::ProtoXep);
        let jingle = accept.get_child("jingle", NS_JINGLE).unwrap();
        assert!(jingle
            .get_child("jingle-participant", NS_JINGLE_PARTICIPANT)
            .is_none());
        let origin = jingle.get_child("origin", NS_AV_CONFERENCES).unwrap();
        assert_eq!(origin.attr("entity"), Some("xmpp:bob@example.org"));
    }

    #[test]
    fn protoxep_origin_is_parsed() {
        let xml = r"<iq xmlns='jabber:client' type='set' from='room@conference.example.org' id='x'><jingle xmlns='urn:xmpp:jingle:1' action='session-initiate' sid='s1' initiator='room@conference.example.org'><origin xmlns='urn:xmpp:jingle:av-conferences:0' entity='xmpp:alice@example.org'/><content creator='initiator' name='0'><description xmlns='urn:xmpp:jingle:apps:rtp:1' media='audio'/></content></jingle></iq>";
        let iq = element(xml);
        let parsed = ConferenceJingle::parse(iq.get_child("jingle", NS_JINGLE).unwrap()).unwrap();
        assert_eq!(parsed.origin.unwrap().to_string(), "alice@example.org");
    }
}
