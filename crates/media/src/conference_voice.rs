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
use xmpp_core::conference::ConferenceSession;
use xmpp_core::voice::{VoiceChannel, VoiceEvent, VoiceJoin, VoiceParticipant, VoicePort};
use xmpp_core::{Jid, XmppClient};

struct State {
    client: XmppClient,
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
    pub fn new(client: XmppClient) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            state: Arc::new(State {
                client,
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
            participants.push(VoiceParticipant {
                id: bare.clone(),
                name: jid.node().map_or_else(|| bare.clone(), ToString::to_string),
                muted: session.muted.get(&bare).copied().unwrap_or(false)
                    || (is_self && self.state.self_muted.load(Ordering::Relaxed)),
                speaking: false,
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

        let session = match self.state.client.start_voice_conference(&room) {
            Ok(session) => session,
            Err(error) => {
                self.state.joined.store(false, Ordering::Relaxed);
                return Err(anyhow::anyhow!(error.to_string()));
            }
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
        if let Some((room, session)) = self.session() {
            self.state.client.leave_voice_conference(&room)?;
            self.state.client.jingle.stop_session_media(&session.sid);
        }
        self.state.room.write().take();
        self.state.channel.write().take();
        *self.state.last.write() = None;
        let _ = self.state.events.send(VoiceEvent::Left);
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
            if let Some(content) = session.contents.values().find(|content| {
                content
                    .participant
                    .as_ref()
                    .is_some_and(|candidate| candidate.to_bare() == participant.to_bare())
            }) {
                self.state
                    .client
                    .jingle
                    .set_content_volume(&session.sid, &content.name, volume);
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
