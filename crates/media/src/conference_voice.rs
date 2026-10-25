//! Voice-channel adapter backed by XMPP SFU conferences (ADR-014).
//!
//! The space UI talks to [`VoicePort`]; this implementation maps that contract
//! onto the room's Jingle conference. Galène configuration is no longer part of
//! the request: the client joins the MUC, discovers the SFU and calls it.
//!
//! Per-participant gain is stored here and in `ConferenceManager`. The media
//! pipeline applies it once the per-content audio branches land; mute already
//! reaches the microphone through the Jingle media backend.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use xmpp_core::conference::{sfu_jid_for, ConferenceSession};
use xmpp_core::voice::{VoiceChannel, VoiceEvent, VoiceJoin, VoiceParticipant, VoicePort};
use xmpp_core::{ConferenceState, Jid, MucAffiliation, XmppClient};

struct State {
    client: XmppClient,
    /// SFU component to invite when a room has none (see `join`).
    sfu: Option<Jid>,
    room: RwLock<Option<Jid>>,
    channel: RwLock<Option<VoiceChannel>>,
    joined: AtomicBool,
    self_muted: AtomicBool,
    last: RwLock<Option<Vec<VoiceParticipant>>>,
    events: broadcast::Sender<VoiceEvent>,
}

/// A [`VoicePort`] that joins a MUC voice room and speaks Jingle to its SFU.
pub struct ConferenceVoicePort {
    state: Arc<State>,
}

impl ConferenceVoicePort {
    #[must_use]
    pub fn new(client: XmppClient, sfu: Option<Jid>) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            state: Arc::new(State {
                client,
                sfu,
                room: RwLock::new(None),
                channel: RwLock::new(None),
                joined: AtomicBool::new(false),
                self_muted: AtomicBool::new(false),
                last: RwLock::new(None),
                events,
            }),
        }
    }

    fn session(&self) -> Option<(Jid, ConferenceSession)> {
        let room = self.state.room.read().clone()?;
        let session = self.state.client.conference.session(&room)?;
        Some((room, session))
    }

    fn snapshot(&self) -> Vec<VoiceParticipant> {
        let Some((room, session)) = self.session() else {
            return Vec::new();
        };
        let own = self
            .state
            .client
            .connection
            .jid()
            .map(|jid| jid.to_bare().to_string());
        let mut participants: Vec<VoiceParticipant> = Vec::new();

        let mut push = |jid: &Jid| {
            let bare = jid.to_bare().to_string();
            if participants.iter().any(|p| p.id == bare) {
                return;
            }
            let is_self = own.as_deref() == Some(bare.as_str());
            let content = session
                .contents
                .values()
                .find(|content| {
                    content.media == "audio"
                        && content
                            .participant
                            .as_ref()
                            .is_some_and(|participant| participant.to_bare() == jid.to_bare())
                })
                .or_else(|| {
                    session.contents.values().find(|content| {
                        content
                            .participant
                            .as_ref()
                            .is_some_and(|participant| participant.to_bare() == jid.to_bare())
                    })
                });
            let screen_sharing = session.contents.values().any(|content| {
                content.category == xmpp_core::conference::ConferenceCategory::Slides
                    && content
                        .participant
                        .as_ref()
                        .is_some_and(|participant| participant.to_bare() == jid.to_bare())
            });
            participants.push(VoiceParticipant {
                id: bare.clone(),
                name: jid.node().map_or_else(|| bare.clone(), ToString::to_string),
                muted: session.muted.get(&bare).copied().unwrap_or(false)
                    || (is_self && self.state.self_muted.load(Ordering::Relaxed)),
                speaking: content.is_some_and(|content| {
                    self.state
                        .client
                        .jingle
                        .content_speaking(&session.sid, &content.stream_key())
                }),
                screen_sharing,
                is_self,
                volume: self.state.client.conference.participant_volume(&room, jid),
            });
        };

        for participant in session.participants.values() {
            push(participant);
        }
        for content in session.contents.values() {
            if let Some(participant) = &content.participant {
                push(participant);
            }
        }
        if let Some(own) = self.state.client.connection.jid() {
            push(&own);
        }

        participants.sort_by(|a, b| b.is_self.cmp(&a.is_self).then(a.name.cmp(&b.name)));
        participants
    }
    /// Appoint a voice room's SFU (and the virtual user it signs in with) as an
    /// admin.
    ///
    /// Galene rejects every call in a semi-anonymous room because it cannot see
    /// the participants' real JIDs; an admin sees them. Only the room owner or
    /// an admin may grant that, hence `allowed`. Returns whether anything was
    /// sent, so the caller can give the server a moment.
    fn appoint_sfu(&self, room: &Jid, sfu: &Jid, allowed: bool) -> bool {
        if !allowed {
            return false;
        }
        if let Err(error) =
            self.state
                .client
                .set_muc_affiliation(room, sfu, "admin", Some("voice channel SFU"))
        {
            tracing::warn!(%room, "could not appoint {sfu} admin: {error}");
            return false;
        }
        // Galener joins as `sha1(room)@<component domain>`, not as the bare
        // component, and that is the JID the propose is addressed to.
        if let Some(hashed) = sfu_jid_for(room, sfu) {
            let _ = self.state.client.set_muc_affiliation(
                room,
                &hashed,
                "admin",
                Some("voice channel SFU"),
            );
        }
        true
    }
}

#[async_trait]
impl VoicePort for ConferenceVoicePort {
    async fn join(&self, request: VoiceJoin) -> anyhow::Result<()> {
        let room: Jid = request
            .channel
            .room
            .parse()
            .map_err(|error| anyhow::anyhow!("invalid voice room JID: {error}"))?;

        *self.state.room.write() = Some(room.clone());
        *self.state.channel.write() = Some(request.channel.clone());
        *self.state.last.write() = None;
        self.state.joined.store(true, Ordering::Relaxed);

        // The SFU is only visible as a MUC occupant — Galener announces itself
        // with a hashed virtual user in the room's presence — so the discovery
        // `start_voice_conference` needs cannot exist until we are in the room.
        // Voice channels are not joined when they are opened, so join here.
        // `is_joined` and not just the entry: saved rooms are restored at
        // startup without a live session, and joining is what makes the server
        // send us the occupant list.
        let in_room = self
            .state
            .client
            .mucs
            .get_by_bare(&room.to_bare().to_string())
            .is_some_and(|muc| muc.is_joined());
        if !in_room {
            if let Err(error) = self.state.client.join_room(&room, &request.username, None) {
                self.state.joined.store(false, Ordering::Relaxed);
                return Err(anyhow::anyhow!("could not join {room}: {error}"));
            }
        }

        // Our own affiliation only arrives with our presence, and appointing
        // the SFU is the room owner's job, so wait for it before deciding.
        for _ in 0..30 {
            let joined = self
                .state
                .client
                .mucs
                .get_by_bare(&room.to_bare().to_string())
                .is_some_and(|muc| muc.is_joined());
            if joined {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let manager = self
            .state
            .client
            .mucs
            .get_by_bare(&room.to_bare().to_string())
            .is_some_and(|muc| {
                matches!(
                    muc.self_affiliation(),
                    MucAffiliation::Owner | MucAffiliation::Admin
                )
            });

        // Galene has to be an admin of the room before it will carry a call:
        // in a semi-anonymous room it cannot see the participants' real JIDs,
        // and it rejects every propose until it can. Appoint the configured
        // component here, before it is invited, so it joins with the
        // affiliation already in place.
        if let Some(sfu) = self.state.sfu.clone() {
            if self.appoint_sfu(&room, &sfu, manager) {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }

        // Joining is what starts the discovery, and the presence that carries
        // it is asynchronous: pressing Join right after the room opens would
        // otherwise always race the SFU and report the room as having none.
        for _ in 0..40 {
            if self.state.client.conference.discovery(&room).is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }

        // Still nothing: a voice channel the client created itself never had an
        // SFU invited, and Prosody does not list the component under disco, so
        // the JID can only come from configuration. Invite it and give Galener
        // the same window to join before reporting the room as having none.
        if self.state.client.conference.discovery(&room).is_none() {
            if let Some(sfu) = self.state.sfu.clone() {
                let _ = self
                    .state
                    .client
                    .invite_to_muc(&room, &sfu, Some("voice channel"));
                for _ in 0..40 {
                    if self.state.client.conference.discovery(&room).is_some() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
        }

        // A room that already had an SFU before this client appointed it is
        // repaired here: the discovered virtual user names the component.
        if self.state.sfu.is_none() {
            if let Some(discovery) = self.state.client.conference.discovery(&room) {
                if self.appoint_sfu(&room, &discovery.jid, manager) {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }

        // A refused propose leaves a session the UI would still count as
        // joined, and a call that is still setting up its media is not a
        // failure: wait for `Active`, retry an explicit refusal (a guest can
        // press Join a moment before the room owner has appointed the SFU), and
        // accept a call that is still connecting when the wait runs out.
        // Silence is not retried, because the unanswered session would stay
        // behind and be the one every later lookup returns.
        let mut joined = None;
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            let session = match self.state.client.start_voice_conference(&room) {
                Ok(session) => session,
                Err(error) => {
                    self.state.joined.store(false, Ordering::Relaxed);
                    return Err(anyhow::anyhow!(error.to_string()));
                }
            };
            let mut refused = false;
            let mut connecting = false;
            for _ in 0..40 {
                match self
                    .state
                    .client
                    .conference
                    .session(&room)
                    .map(|session| session.state)
                {
                    Some(ConferenceState::Active) => {
                        joined = Some(session.clone());
                        break;
                    }
                    Some(ConferenceState::Connecting) => connecting = true,
                    None | Some(ConferenceState::Ended) => {
                        refused = true;
                        break;
                    }
                    _ => {}
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            if joined.is_some() {
                break;
            }
            if connecting {
                joined = Some(session);
                break;
            }
            if !refused {
                break;
            }
        }
        let Some(session) = joined else {
            self.state.joined.store(false, Ordering::Relaxed);
            return Err(anyhow::anyhow!("the SFU did not accept the call in {room}"));
        };

        if request.muted {
            self.state
                .client
                .jingle
                .set_muted(&session.sid, true)
                .await?;
            self.state.self_muted.store(true, Ordering::Relaxed);
        }

        let _ = self.state.events.send(VoiceEvent::Joined);
        let state = self.state.clone();
        tokio::spawn(async move {
            loop {
                if !state.joined.load(Ordering::Relaxed) {
                    return;
                }
                let port = Self {
                    state: state.clone(),
                };
                let participants = port.snapshot();
                let changed = state.last.read().as_ref() != Some(&participants);
                if changed {
                    *state.last.write() = Some(participants.clone());
                    let _ = state.events.send(VoiceEvent::Participants(participants));
                }

                let ended = state
                    .room
                    .read()
                    .clone()
                    .is_some_and(|room| state.client.conference.session(&room).is_none());
                if ended {
                    state.joined.store(false, Ordering::Relaxed);
                    let _ = state.events.send(VoiceEvent::Left);
                    return;
                }

                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
        });

        Ok(())
    }

    async fn leave(&self) -> anyhow::Result<()> {
        self.state.joined.store(false, Ordering::Relaxed);
        if let Some((room, _)) = self.session() {
            self.state.client.leave_voice_conference(&room)?;
        }
        self.state.room.write().take();
        self.state.channel.write().take();
        *self.state.last.write() = None;
        let _ = self.state.events.send(VoiceEvent::Left);
        Ok(())
    }

    async fn set_screen_sharing(&self, enabled: bool) -> anyhow::Result<()> {
        let Some((room, _)) = self.session() else {
            return Err(anyhow::anyhow!("not in a voice channel"));
        };
        if enabled {
            self.state.client.start_voice_screen(&room).await?;
        } else {
            self.state.client.stop_voice_screen(&room).await?;
        }
        Ok(())
    }

    async fn set_muted(&self, muted: bool) -> anyhow::Result<()> {
        let Some((_, session)) = self.session() else {
            return Err(anyhow::anyhow!("not in a voice channel"));
        };
        self.state
            .client
            .jingle
            .set_muted(&session.sid, muted)
            .await?;
        self.state.self_muted.store(muted, Ordering::Relaxed);
        let participants = self.snapshot();
        *self.state.last.write() = Some(participants.clone());
        let _ = self
            .state
            .events
            .send(VoiceEvent::Participants(participants));
        Ok(())
    }

    async fn set_participant_volume(&self, id: &str, volume: f64) -> anyhow::Result<()> {
        let Some((room, _)) = self.session() else {
            return Err(anyhow::anyhow!("not in a voice channel"));
        };
        let participant: Jid = id
            .parse()
            .map_err(|error| anyhow::anyhow!("invalid participant JID: {error}"))?;
        let session = self.state.client.conference.session(&room);
        if let Some(session) = session {
            let content =
                session
                    .contents
                    .values()
                    .find(|content| {
                        content.media == "audio"
                            && content.participant.as_ref().is_some_and(|candidate| {
                                candidate.to_bare() == participant.to_bare()
                            })
                    })
                    .or_else(|| {
                        session.contents.values().find(|content| {
                            content.participant.as_ref().is_some_and(|candidate| {
                                candidate.to_bare() == participant.to_bare()
                            })
                        })
                    });
            if let Some(content) = content {
                self.state.client.jingle.set_content_volume(
                    &session.sid,
                    &content.stream_key(),
                    volume,
                );
            }
        }
        self.state
            .client
            .conference
            .set_participant_volume(&room, &participant, volume);
        let participants = self.snapshot();
        *self.state.last.write() = Some(participants.clone());
        let _ = self
            .state
            .events
            .send(VoiceEvent::Participants(participants));
        Ok(())
    }

    fn is_joined(&self) -> bool {
        self.state.joined.load(Ordering::Relaxed) && self.session().is_some()
    }

    fn channel(&self) -> Option<VoiceChannel> {
        self.state.channel.read().clone()
    }

    fn participants(&self) -> Vec<VoiceParticipant> {
        self.snapshot()
    }

    fn subscribe(&self) -> broadcast::Receiver<VoiceEvent> {
        self.state.events.subscribe()
    }
}
