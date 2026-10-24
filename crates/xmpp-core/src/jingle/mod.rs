//! XEP-0166 Jingle, XEP-0353 Jingle Message Initiation and the call session
//! state machine shared by every media backend.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use jid::Jid;
use minidom::Element;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use xmpp_parsers::ns;

use crate::{Connection, EventBus, IqRouter, Result, XmppError, XmppEvent};

pub const NS_JINGLE: &str = "urn:xmpp:jingle:1";
pub const NS_JINGLE_MESSAGE: &str = "urn:xmpp:jingle-message:0";
pub const NS_JINGLE_RTP: &str = "urn:xmpp:jingle:apps:rtp:1";
pub const NS_JINGLE_RTP_AUDIO: &str = "urn:xmpp:jingle:apps:rtp:audio";
pub const NS_JINGLE_RTP_VIDEO: &str = "urn:xmpp:jingle:apps:rtp:video";
pub const NS_JINGLE_ICE_UDP: &str = "urn:xmpp:jingle:transports:ice-udp:1";
pub const NS_JINGLE_DTLS: &str = "urn:xmpp:jingle:apps:dtls:0";
pub const NS_JINGLE_GROUPING: &str = "urn:xmpp:jingle:apps:grouping:0";

/// The media carried by a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallKind {
    Audio,
    Video,
}

impl CallKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Video => "video",
        }
    }

    #[must_use]
    pub fn from_description(description: &Element) -> Self {
        if description.attr("media") == Some("video") {
            Self::Video
        } else {
            Self::Audio
        }
    }
}

/// Who started the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallDirection {
    Incoming,
    Outgoing,
}

/// The lifecycle of a call session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallState {
    Proposing,
    Ringing,
    Incoming,
    Accepted,
    Connecting,
    Active,
    Ended,
    Rejected,
    Missed,
    Failed,
}

impl CallState {
    #[must_use]
    pub const fn is_final(self) -> bool {
        matches!(
            self,
            Self::Ended | Self::Rejected | Self::Missed | Self::Failed
        )
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposing => "proposing",
            Self::Ringing => "ringing",
            Self::Incoming => "incoming",
            Self::Accepted => "accepted",
            Self::Connecting => "connecting",
            Self::Active => "active",
            Self::Ended => "ended",
            Self::Rejected => "rejected",
            Self::Missed => "missed",
            Self::Failed => "failed",
        }
    }
}

/// A call session as seen by the protocol layer.
#[derive(Debug, Clone)]
pub struct CallSession {
    pub sid: String,
    pub peer: Jid,
    pub peer_full: Option<Jid>,
    pub direction: CallDirection,
    pub kind: CallKind,
    pub camera_enabled: bool,
    pub camera_pending: bool,
    pub camera_error: Option<String>,
    pub screen_sharing: bool,
    pub screen_pending: bool,
    pub remote_screen: bool,
    pub screen_error: Option<String>,
    pub screen_content: Option<String>,
    pub state: CallState,
    pub created_at: DateTime<Utc>,
    pub connected_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub reason: Option<String>,
    pub offer: Option<String>,
    pub answer: Option<String>,
}

/// A local camera that can be selected for outgoing video.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraDevice {
    pub id: String,
    pub name: String,
}

/// One local audio input device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

/// One locally captured camera frame, JPEG encoded for the UI.
#[derive(Clone, Debug)]
pub struct CallPreview {
    pub width: u32,
    pub height: u32,
    pub jpeg: Arc<Vec<u8>>,
    pub sequence: u64,
}

/// Events emitted by a media backend.
#[derive(Debug, Clone)]
pub enum MediaEvent {
    LocalCandidate {
        sid: String,
        mline: u32,
        jingle: String,
    },
    Connected {
        sid: String,
    },
    Disconnected {
        sid: String,
    },
    Error {
        sid: String,
        message: String,
    },
}

/// The media plane contract. The protocol layer never touches audio/video.
#[async_trait]
pub trait MediaPort: Send + Sync {
    async fn create_offer(&self, sid: &str, kind: CallKind) -> anyhow::Result<String>;
    async fn accept_offer(&self, sid: &str, kind: CallKind, offer: &str) -> anyhow::Result<String>;
    async fn apply_answer(&self, sid: &str, answer: &str) -> anyhow::Result<()>;
    async fn add_candidate(&self, sid: &str, candidate: &str) -> anyhow::Result<()>;
    async fn set_muted(&self, sid: &str, muted: bool) -> anyhow::Result<()>;
    async fn set_speaker_muted(&self, sid: &str, muted: bool) -> anyhow::Result<()>;
    async fn set_camera(&self, sid: &str, enabled: bool) -> anyhow::Result<()>;
    fn preview_frame(&self, sid: &str) -> Option<CallPreview>;
    fn remote_preview_frame(&self, sid: &str) -> Option<CallPreview>;
    fn camera_devices(&self) -> Vec<CameraDevice>;
    fn active_camera_device(&self, sid: &str) -> Option<String>;
    fn set_preferred_camera(&self, device_id: Option<&str>);
    fn audio_devices(&self) -> Vec<AudioDevice>;
    fn set_preferred_audio(&self, device_id: Option<&str>);
    async fn set_camera_device(&self, sid: &str, device_id: Option<&str>) -> anyhow::Result<()>;
    async fn add_video_offer(&self, sid: &str) -> anyhow::Result<String>;
    async fn accept_video_add(&self, sid: &str, content_add: &str) -> anyhow::Result<String>;
    async fn apply_video_answer(&self, sid: &str, content_accept: &str) -> anyhow::Result<()>;
    async fn add_screen_offer(&self, sid: &str) -> anyhow::Result<String>;
    async fn accept_screen_add(&self, sid: &str, content_add: &str) -> anyhow::Result<String>;
    async fn apply_screen_answer(&self, sid: &str, content_accept: &str) -> anyhow::Result<()>;
    async fn set_screen(&self, sid: &str, enabled: bool) -> anyhow::Result<()>;
    /// Accept a remote conference content (an SFU participant's stream).
    ///
    /// Backends that have no SFU receive path keep the default error.
    async fn accept_remote_content(
        &self,
        _sid: &str,
        _content_add: &str,
    ) -> anyhow::Result<String> {
        Err(anyhow::anyhow!(
            "this media backend does not support conference contents"
        ))
    }
    fn screen_preview_frame(&self, sid: &str) -> Option<CallPreview>;
    /// Set the playback gain of one conference content.
    fn set_content_volume(&self, _sid: &str, _content: &str, _gain: f64) {}
    /// Whether a conference content carried voice recently.
    fn content_speaking(&self, _sid: &str, _content: &str) -> bool {
        false
    }
    fn remote_screen_frame(&self, sid: &str) -> Option<CallPreview>;
    fn clear_remote_screen(&self, sid: &str);
    async fn stop(&self, sid: &str) -> anyhow::Result<()>;
    fn is_connected(&self, sid: &str) -> bool;
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<MediaEvent>;
}

/// Jingle signaling and call state.
pub struct JingleManager {
    connection: Arc<Connection>,
    #[allow(dead_code)]
    iq: Arc<IqRouter>,
    events: Arc<EventBus>,
    media: RwLock<Option<Arc<dyn MediaPort>>>,
    sessions: RwLock<HashMap<String, CallSession>>,
    own_jid: RwLock<Option<Jid>>,
}

impl JingleManager {
    #[must_use]
    pub fn new(connection: Arc<Connection>, iq: Arc<IqRouter>, events: Arc<EventBus>) -> Arc<Self> {
        Arc::new(Self {
            connection,
            iq,
            events,
            media: RwLock::new(None),
            sessions: RwLock::new(HashMap::new()),
            own_jid: RwLock::new(None),
        })
    }

    pub fn set_media(&self, media: Arc<dyn MediaPort>) {
        *self.media.write() = Some(media);
    }

    /// Forward media events into the call state. Call once the async runtime exists.
    pub fn start_media_listener(self: &Arc<Self>) {
        let Some(media) = self.media.read().clone() else {
            return;
        };
        let mut events = media.subscribe();
        let manager = self.clone();
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                match event {
                    MediaEvent::LocalCandidate { sid, jingle, .. } => {
                        if let Some(session) = manager.session(&sid) {
                            let to = session.peer_full.unwrap_or(session.peer);
                            let _ = manager.send_jingle_iq(&to, &jingle);
                        }
                    }
                    MediaEvent::Connected { sid } => {
                        tracing::info!("call {sid} media connected");
                        let camera_enabled = {
                            let mut sessions = manager.sessions.write();
                            match sessions.get_mut(&sid) {
                                Some(session) => {
                                    session.connected_at = Some(Utc::now());
                                    session.state = CallState::Active;
                                    session.camera_enabled
                                }
                                None => false,
                            }
                        };
                        if camera_enabled {
                            let media = manager.media.read().clone();
                            if let Some(media) = media {
                                let _ = media.set_camera(&sid, true).await;
                            }
                        }
                        manager.emit_changed(&sid);
                    }
                    MediaEvent::Disconnected { sid } => {
                        if let Some(session) = manager.session(&sid) {
                            if !session.state.is_final() {
                                manager.stop_media(&sid);
                                manager.finish_session(
                                    &sid,
                                    CallState::Ended,
                                    Some("disconnected"),
                                );
                            }
                        }
                    }
                    MediaEvent::Error { sid, message } => {
                        manager.stop_media(&sid);
                        manager.finish_session(&sid, CallState::Failed, Some(&message));
                    }
                }
            }
        });
    }

    pub fn set_own_jid(&self, jid: Jid) {
        *self.own_jid.write() = Some(jid);
    }

    /// End every session without sending anything (disconnect/reconnect).
    pub fn clear(&self) {
        let sessions: Vec<CallSession> = self.sessions.read().values().cloned().collect();
        for session in sessions {
            if let Some(media) = self.media.read().clone() {
                let sid = session.sid.clone();
                tokio::spawn(async move {
                    let _ = media.stop(&sid).await;
                });
            }
            self.finish_session(&session.sid, CallState::Failed, Some("disconnected"));
        }
    }

    #[must_use]
    pub fn session(&self, sid: &str) -> Option<CallSession> {
        self.sessions.read().get(sid).cloned()
    }

    #[must_use]
    pub fn sessions(&self) -> Vec<CallSession> {
        let mut sessions: Vec<CallSession> = self.sessions.read().values().cloned().collect();
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        sessions
    }

    #[must_use]
    pub fn active_session_for(&self, peer: &Jid) -> Option<CallSession> {
        let bare = peer.to_bare();
        self.sessions
            .read()
            .values()
            .find(|session| session.peer.to_bare() == bare && !session.state.is_final())
            .cloned()
    }

    /// Start an outgoing call and send XEP-0353 `propose`.
    pub async fn start_call(self: &Arc<Self>, peer: &Jid, kind: CallKind) -> Result<String> {
        self.prune_finished();
        if let Some(existing) = self.active_session_for(peer) {
            return Err(XmppError::ProtocolError(format!(
                "already in a call with {} ({})",
                peer.to_bare(),
                existing.sid
            )));
        }

        let sid = Uuid::new_v4().to_string();
        let session = CallSession {
            sid: sid.clone(),
            peer: peer.to_bare().into(),
            peer_full: Some(peer.clone()).filter(|jid| jid.resource().is_some()),
            direction: CallDirection::Outgoing,
            kind,
            camera_enabled: kind == CallKind::Video,
            camera_pending: false,
            camera_error: None,
            screen_sharing: false,
            screen_pending: false,
            remote_screen: false,
            screen_error: None,
            screen_content: None,
            state: CallState::Proposing,
            created_at: Utc::now(),
            connected_at: None,
            ended_at: None,
            reason: None,
            offer: None,
            answer: None,
        };
        self.sessions.write().insert(sid.clone(), session);
        self.emit_changed(&sid);

        let description = propose_description(kind);
        let propose = xmpp_parsers::jingle_message::JingleMI::Propose {
            sid: xmpp_parsers::jingle::SessionId(sid.clone()),
            description,
        };
        let message = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", peer.to_string())
            .attr("id", format!("jmi-{sid}"))
            .append(Element::from(propose))
            .build();
        self.connection.send_stanza_now(message)?;
        self.spawn_ring_timeout(sid.clone(), true);
        Ok(sid)
    }

    /// Accept an incoming call. The caller then sends `session-initiate`.
    pub async fn accept_call(&self, sid: &str) -> Result<()> {
        let session = self.require_session(sid)?;
        if session.state != CallState::Incoming {
            return Err(XmppError::ProtocolError("call is not ringing".into()));
        }
        let to = session
            .peer_full
            .clone()
            .unwrap_or_else(|| session.peer.clone());
        self.send_jmi(
            &to,
            xmpp_parsers::jingle_message::JingleMI::Accept(xmpp_parsers::jingle::SessionId(
                sid.to_string(),
            )),
        )?;
        self.set_state(sid, CallState::Accepted, None);
        Ok(())
    }

    /// Reject an incoming call, or cancel an outgoing one.
    pub async fn reject_call(&self, sid: &str, reason: Option<&str>) -> Result<()> {
        let session = self.require_session(sid)?;
        let to = session
            .peer_full
            .clone()
            .unwrap_or_else(|| session.peer.clone());
        match session.direction {
            CallDirection::Incoming => {
                self.send_jmi(
                    &to,
                    xmpp_parsers::jingle_message::JingleMI::Reject(
                        xmpp_parsers::jingle::SessionId(sid.to_string()),
                    ),
                )?;
                self.stop_media(sid);
                self.finish_session(sid, CallState::Rejected, reason);
            }
            CallDirection::Outgoing => {
                self.send_jmi(
                    &to,
                    xmpp_parsers::jingle_message::JingleMI::Retract(
                        xmpp_parsers::jingle::SessionId(sid.to_string()),
                    ),
                )?;
                self.stop_media(sid);
                self.finish_session(sid, CallState::Ended, reason);
            }
        }
        Ok(())
    }

    /// Hang up an established (or connecting) call.
    pub async fn end_call(&self, sid: &str, reason: Option<&str>) -> Result<()> {
        let session = self.require_session(sid)?;
        let to = session
            .peer_full
            .clone()
            .unwrap_or_else(|| session.peer.clone());
        if matches!(
            session.state,
            CallState::Incoming | CallState::Accepted | CallState::Proposing | CallState::Ringing
        ) {
            return self.reject_call(sid, reason).await;
        }
        let jingle = build_terminate(&session.sid, reason.unwrap_or("success"));
        let _ = self.send_jingle_iq(&to, &jingle);
        self.stop_media(sid);
        self.finish_session(sid, CallState::Ended, reason);
        Ok(())
    }

    /// Create the media offer for a conference session.
    pub async fn create_conference_offer(&self, sid: &str) -> Result<String> {
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        media
            .create_offer(sid, CallKind::Audio)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))
    }

    /// Install the SFU's `session-accept` answer for a conference session.
    pub async fn apply_conference_answer(&self, sid: &str, answer: &str) -> Result<()> {
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        media
            .apply_answer(sid, answer)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))
    }

    /// Add an SFU `transport-info` candidate to a conference session.
    pub async fn add_conference_candidate(&self, sid: &str, jingle: &str) -> Result<()> {
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        media
            .add_candidate(sid, jingle)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))
    }

    /// Accept one remote conference content and return the `content-accept`.
    pub async fn accept_remote_content(&self, sid: &str, content_add: &str) -> Result<String> {
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        media
            .accept_remote_content(sid, content_add)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))
    }

    pub async fn set_muted(&self, sid: &str, muted: bool) -> Result<()> {
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        media
            .set_muted(sid, muted)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))
    }

    pub async fn set_speaker_muted(&self, sid: &str, muted: bool) -> Result<()> {
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        media
            .set_speaker_muted(sid, muted)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))
    }

    pub async fn set_camera(self: &Arc<Self>, sid: &str, enabled: bool) -> Result<()> {
        let session = self.require_session(sid)?;
        match session.kind {
            CallKind::Video => self.apply_camera(sid, enabled).await,
            CallKind::Audio if enabled => self.upgrade_to_video(sid).await,
            CallKind::Audio => Ok(()),
        }
    }

    async fn apply_camera(&self, sid: &str, enabled: bool) -> Result<()> {
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        media
            .set_camera(sid, enabled)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))?;
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.camera_enabled = enabled;
            session.camera_pending = false;
            session.camera_error = None;
        }
        self.emit_changed(sid);
        Ok(())
    }

    /// Add video to an active audio call via Jingle `content-add`.
    pub async fn upgrade_to_video(self: &Arc<Self>, sid: &str) -> Result<()> {
        let session = self.require_session(sid)?;
        if session.kind == CallKind::Video {
            return self.apply_camera(sid, true).await;
        }
        if session.state != CallState::Active {
            return Err(XmppError::ProtocolError(
                "camera can only be enabled on an active call".into(),
            ));
        }
        if session.camera_pending {
            return Ok(());
        }
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.camera_pending = true;
            session.camera_error = None;
        }
        self.emit_changed(sid);
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        let content_add = match media.add_video_offer(sid).await {
            Ok(content_add) => set_content_creator(
                &content_add,
                match session.direction {
                    CallDirection::Outgoing => "initiator",
                    CallDirection::Incoming => "responder",
                },
            ),
            Err(error) => {
                if let Some(session) = self.sessions.write().get_mut(sid) {
                    session.camera_pending = false;
                    session.camera_error = Some(error.to_string());
                }
                self.emit_changed(sid);
                return Err(XmppError::ProtocolError(error.to_string()));
            }
        };
        let to = session
            .peer_full
            .clone()
            .unwrap_or_else(|| session.peer.clone());
        if let Err(error) = self.send_jingle_iq(&to, &content_add) {
            self.stop_media(sid);
            if let Some(session) = self.sessions.write().get_mut(sid) {
                session.camera_pending = false;
                session.camera_error = Some(error.to_string());
            }
            self.emit_changed(sid);
            return Err(error);
        }
        let manager = self.clone();
        let watching = sid.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            let timed_out = {
                let sessions = manager.sessions.read();
                sessions.get(&watching).is_some_and(|session| {
                    session.camera_pending && session.kind == CallKind::Audio
                })
            };
            if !timed_out {
                return;
            }
            let media = manager.media.read().clone();
            if let Some(media) = media {
                let _ = media.set_camera(&watching, false).await;
            }
            if let Some(session) = manager.sessions.write().get_mut(&watching) {
                session.camera_pending = false;
                session.camera_error = Some("camera request timed out".into());
            }
            manager.emit_changed(&watching);
        });
        Ok(())
    }

    async fn on_content_add(
        &self,
        sid: &str,
        from: &Jid,
        jingle: &xmpp_parsers::jingle::Jingle,
    ) -> Result<()> {
        let Some(session) = self.session(sid) else {
            return Ok(());
        };
        if jingle.contents.iter().any(is_screen_content) {
            return self.on_screen_add(sid, from, jingle).await;
        }
        let has_video = jingle.contents.iter().any(|content| {
            matches!(
                &content.description,
                Some(xmpp_parsers::jingle::Description::Rtp(rtp)) if rtp.media == "video"
            )
        });
        if !has_video || session.state != CallState::Active {
            let reject = build_content_reject(sid, jingle);
            let _ = self.send_jingle_iq(from, &reject);
            return Ok(());
        }
        if session.kind == CallKind::Video {
            let reject = build_content_reject(sid, jingle);
            let _ = self.send_jingle_iq(from, &reject);
            return Ok(());
        }
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        let xml = element_to_string(Element::from(jingle.clone()));
        let accept = match media.accept_video_add(sid, &xml).await {
            Ok(accept) => {
                let creator = match jingle.contents.first().map(|content| &content.creator) {
                    Some(xmpp_parsers::jingle::Creator::Responder) => "responder",
                    _ => "initiator",
                };
                set_content_creator(&accept, creator)
            }
            Err(error) => {
                let reject = build_content_reject(sid, jingle);
                let _ = self.send_jingle_iq(from, &reject);
                if let Some(session) = self.sessions.write().get_mut(sid) {
                    session.camera_pending = false;
                    session.camera_error = Some(error.to_string());
                }
                self.emit_changed(sid);
                return Ok(());
            }
        };
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.kind = CallKind::Video;
            session.camera_enabled = false;
            session.camera_pending = false;
            session.camera_error = None;
        }
        self.emit_changed(sid);
        let _ = self.send_jingle_iq(from, &accept);
        Ok(())
    }

    async fn on_screen_add(
        &self,
        sid: &str,
        from: &Jid,
        jingle: &xmpp_parsers::jingle::Jingle,
    ) -> Result<()> {
        let Some(session) = self.session(sid) else {
            return Ok(());
        };
        if session.state != CallState::Active || session.remote_screen {
            let reject = build_content_reject(sid, jingle);
            let _ = self.send_jingle_iq(from, &reject);
            return Ok(());
        }
        let Some(content) = jingle
            .contents
            .iter()
            .find(|content| is_screen_content(content))
        else {
            let reject = build_content_reject(sid, jingle);
            let _ = self.send_jingle_iq(from, &reject);
            return Ok(());
        };
        let creator = content_creator(&content.creator);
        let senders = content_senders(&content.senders);
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        let xml = element_to_string(Element::from(jingle.clone()));
        let accept = match media.accept_screen_add(sid, &xml).await {
            Ok(accept) => mark_screen_content(&accept, creator, senders),
            Err(error) => {
                let reject = build_content_reject(sid, jingle);
                let _ = self.send_jingle_iq(from, &reject);
                if let Some(session) = self.sessions.write().get_mut(sid) {
                    session.screen_error = Some(error.to_string());
                }
                self.emit_changed(sid);
                return Ok(());
            }
        };
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.kind = CallKind::Video;
            session.remote_screen = true;
            session.screen_error = None;
        }
        self.emit_changed(sid);
        let _ = self.send_jingle_iq(from, &accept);
        Ok(())
    }

    /// Start sharing the local screen on an active call.
    pub async fn start_screen_share(self: &Arc<Self>, sid: &str) -> Result<()> {
        let session = self.require_session(sid)?;
        if session.state != CallState::Active {
            return Err(XmppError::ProtocolError(
                "the screen can only be shared on an active call".into(),
            ));
        }
        if session.screen_sharing {
            return Ok(());
        }
        if session.screen_pending {
            return Err(XmppError::ProtocolError(
                "screen sharing is already starting".into(),
            ));
        }
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        if session.screen_content.is_some() {
            media
                .set_screen(sid, true)
                .await
                .map_err(|error| XmppError::ProtocolError(error.to_string()))?;
            if let Some(session) = self.sessions.write().get_mut(sid) {
                session.screen_sharing = true;
                session.screen_error = None;
            }
            self.emit_changed(sid);
            return Ok(());
        }
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.screen_pending = true;
            session.screen_error = None;
        }
        self.emit_changed(sid);
        let (creator, senders) = match session.direction {
            CallDirection::Outgoing => ("initiator", "initiator"),
            CallDirection::Incoming => ("responder", "responder"),
        };
        let content_add = match media.add_screen_offer(sid).await {
            Ok(content_add) => mark_screen_content(&content_add, creator, senders),
            Err(error) => {
                if let Some(session) = self.sessions.write().get_mut(sid) {
                    session.screen_pending = false;
                    session.screen_error = Some(error.to_string());
                }
                self.emit_changed(sid);
                return Err(XmppError::ProtocolError(error.to_string()));
            }
        };
        if let Some(name) = content_name(&content_add) {
            if let Some(session) = self.sessions.write().get_mut(sid) {
                session.screen_content = Some(name);
            }
        }
        let to = session
            .peer_full
            .clone()
            .unwrap_or_else(|| session.peer.clone());
        if let Err(error) = self.send_jingle_iq(&to, &content_add) {
            if let Some(session) = self.sessions.write().get_mut(sid) {
                session.screen_pending = false;
                session.screen_error = Some(error.to_string());
            }
            self.emit_changed(sid);
            return Err(error);
        }
        let manager = self.clone();
        let watching = sid.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let timed_out = manager
                .session(&watching)
                .is_some_and(|session| session.screen_pending);
            if !timed_out {
                return;
            }
            let media = manager.media.read().clone();
            if let Some(media) = media {
                let _ = media.set_screen(&watching, false).await;
            }
            if let Some(session) = manager.sessions.write().get_mut(&watching) {
                session.screen_pending = false;
                session.screen_error = Some("screen share request timed out".into());
            }
            manager.emit_changed(&watching);
        });
        Ok(())
    }

    fn watch_screen_frames(self: &Arc<Self>, sid: &str) {
        let manager = self.clone();
        let watching = sid.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let sharing = manager
                .session(&watching)
                .is_some_and(|session| session.screen_sharing);
            if !sharing || manager.screen_preview_frame(&watching).is_some() {
                return;
            }
            if let Some(session) = manager.sessions.write().get_mut(&watching) {
                session.screen_error = Some("screen capture produced no frames".into());
            }
            manager.emit_changed(&watching);
        });
    }

    /// Stop sharing the local screen.
    pub async fn stop_screen_share(&self, sid: &str) -> Result<()> {
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        media
            .set_screen(sid, false)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))?;
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.screen_sharing = false;
            session.screen_pending = false;
            session.screen_error = None;
        }
        self.emit_changed(sid);
        Ok(())
    }

    async fn on_content_remove(&self, sid: &str, jingle: &xmpp_parsers::jingle::Jingle) {
        if !jingle.contents.iter().any(is_screen_content) {
            return;
        }
        let media = self.media.read().clone();
        if let Some(media) = media {
            let _ = media.set_screen(sid, false).await;
            media.clear_remote_screen(sid);
        }
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.screen_sharing = false;
            session.screen_pending = false;
            session.remote_screen = false;
            session.screen_content = None;
        }
        self.emit_changed(sid);
    }

    #[must_use]
    pub fn screen_preview_frame(&self, sid: &str) -> Option<CallPreview> {
        self.media
            .read()
            .as_ref()
            .and_then(|media| media.screen_preview_frame(sid))
    }

    #[must_use]
    pub fn remote_screen_frame(&self, sid: &str) -> Option<CallPreview> {
        self.media
            .read()
            .as_ref()
            .and_then(|media| media.remote_screen_frame(sid))
    }

    async fn on_content_accept(
        self: &Arc<Self>,
        sid: &str,
        from: &Jid,
        jingle: &xmpp_parsers::jingle::Jingle,
    ) -> Result<()> {
        let Some(session) = self.session(sid) else {
            return Ok(());
        };
        if jingle.contents.iter().any(is_screen_content) {
            if !session.screen_pending {
                return Ok(());
            }
            let media = self
                .media
                .read()
                .clone()
                .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
            let xml = element_to_string(Element::from(jingle.clone()));
            match media.apply_screen_answer(sid, &xml).await {
                Ok(()) => match media.set_screen(sid, true).await {
                    Ok(()) => {
                        if let Some(session) = self.sessions.write().get_mut(sid) {
                            session.kind = CallKind::Video;
                            session.screen_sharing = true;
                            session.screen_pending = false;
                            session.screen_error = None;
                        }
                        self.emit_changed(sid);
                        self.watch_screen_frames(sid);
                    }
                    Err(error) => {
                        if let Some(session) = self.sessions.write().get_mut(sid) {
                            session.screen_pending = false;
                            session.screen_sharing = false;
                            session.screen_error = Some(error.to_string());
                        }
                        self.emit_changed(sid);
                    }
                },
                Err(error) => {
                    if let Some(session) = self.sessions.write().get_mut(sid) {
                        session.screen_pending = false;
                        session.screen_error = Some(error.to_string());
                    }
                    self.emit_changed(sid);
                }
            }
            return Ok(());
        }
        if !session.camera_pending {
            return Ok(());
        }
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        let xml = element_to_string(Element::from(jingle.clone()));
        match media.apply_video_answer(sid, &xml).await {
            Ok(()) => {
                let _ = media.set_camera(sid, true).await;
                if let Some(session) = self.sessions.write().get_mut(sid) {
                    session.kind = CallKind::Video;
                    session.camera_enabled = true;
                    session.camera_pending = false;
                    session.camera_error = None;
                }
                self.emit_changed(sid);
            }
            Err(error) => {
                let _ = media.set_camera(sid, false).await;
                if let Some(session) = self.sessions.write().get_mut(sid) {
                    session.camera_enabled = false;
                    session.camera_pending = false;
                    session.camera_error = Some(error.to_string());
                }
                self.emit_changed(sid);
                let _ = self.send_jingle_iq(from, &build_terminate(sid, "media-error"));
            }
        }
        Ok(())
    }

    async fn on_content_reject(&self, sid: &str) {
        let screen = self
            .session(sid)
            .is_some_and(|session| session.screen_pending);
        let media = self.media.read().clone();
        if !screen {
            if let Some(media) = media {
                let _ = media.set_camera(sid, false).await;
            }
        }
        if let Some(session) = self.sessions.write().get_mut(sid) {
            if screen {
                session.screen_pending = false;
                session.screen_error = Some("the other side declined screen sharing".into());
            } else {
                session.camera_enabled = false;
                session.camera_pending = false;
                if session.kind == CallKind::Audio {
                    session.camera_error = Some("the other side declined the camera".into());
                }
            }
        }
        self.emit_changed(sid);
    }

    #[must_use]
    pub fn camera_devices(&self) -> Vec<CameraDevice> {
        self.media
            .read()
            .as_ref()
            .map(|media| media.camera_devices())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn audio_devices(&self) -> Vec<AudioDevice> {
        self.media
            .read()
            .as_ref()
            .map(|media| media.audio_devices())
            .unwrap_or_default()
    }

    /// Remember the microphone used for future calls.
    pub fn set_audio_preference(&self, device_id: Option<&str>) {
        if let Some(media) = self.media.read().clone() {
            media.set_preferred_audio(device_id);
        }
    }

    /// Remember the camera used for future calls.
    pub fn set_camera_preference(&self, device_id: Option<&str>) {
        if let Some(media) = self.media.read().clone() {
            media.set_preferred_camera(device_id);
        }
    }

    /// Remember the camera used for future calls and switch active video calls.
    pub async fn set_preferred_camera(&self, device_id: Option<&str>) -> Result<()> {
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        media.set_preferred_camera(device_id);
        let sessions: Vec<String> = self
            .sessions
            .read()
            .values()
            .filter(|session| session.kind == CallKind::Video && !session.state.is_final())
            .map(|session| session.sid.clone())
            .collect();
        for sid in sessions {
            media
                .set_camera_device(&sid, device_id)
                .await
                .map_err(|error| XmppError::ProtocolError(error.to_string()))?;
        }
        Ok(())
    }

    pub async fn set_camera_device(&self, sid: &str, device_id: Option<&str>) -> Result<()> {
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        media
            .set_camera_device(sid, device_id)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))
    }

    /// Switch to the next available camera.
    pub async fn switch_camera(&self, sid: &str) -> Result<()> {
        let devices = self.camera_devices();
        if devices.len() < 2 {
            return Err(XmppError::ProtocolError(
                "no other camera is available".into(),
            ));
        }
        let current = self
            .media
            .read()
            .as_ref()
            .and_then(|media| media.active_camera_device(sid));
        let index = current
            .as_ref()
            .and_then(|current| devices.iter().position(|device| &device.id == current))
            .map_or(0, |index| index + 1);
        let device = &devices[index % devices.len()];
        self.set_camera_device(sid, Some(&device.id)).await
    }

    #[must_use]
    pub fn preview_frame(&self, sid: &str) -> Option<CallPreview> {
        self.media
            .read()
            .as_ref()
            .and_then(|media| media.preview_frame(sid))
    }

    #[must_use]
    pub fn remote_preview_frame(&self, sid: &str) -> Option<CallPreview> {
        self.media
            .read()
            .as_ref()
            .and_then(|media| media.remote_preview_frame(sid))
    }

    /// Handle an incoming Jingle IQ. Returns the IQ result to send back.
    pub async fn handle_jingle_iq(self: &Arc<Self>, elem: &Element) -> Result<Option<Element>> {
        let Some(child) = elem.get_child("jingle", NS_JINGLE) else {
            return Ok(None);
        };
        let jingle = xmpp_parsers::jingle::Jingle::try_from(child.clone())
            .map_err(|error| XmppError::ParseError(error.to_string()))?;
        let sid = jingle.sid.0.clone();
        let from = elem
            .attr("from")
            .and_then(|from| from.parse::<Jid>().ok())
            .ok_or_else(|| XmppError::ProtocolError("jingle IQ without from".into()))?;

        match jingle.action {
            xmpp_parsers::jingle::Action::SessionInitiate => {
                self.on_session_initiate(&sid, &from, child.clone(), &jingle)
                    .await?;
            }
            xmpp_parsers::jingle::Action::SessionAccept => {
                self.on_session_accept(&sid, &from, child.clone(), &jingle)
                    .await?;
            }
            xmpp_parsers::jingle::Action::SessionTerminate => {
                let reason = jingle
                    .reason
                    .as_ref()
                    .map(|reason| Element::from(reason.reason.clone()).name().to_string())
                    .unwrap_or_else(|| "ended".to_string());
                self.stop_media(&sid);
                self.finish_session(&sid, CallState::Ended, Some(&reason));
            }
            xmpp_parsers::jingle::Action::ContentAdd => {
                self.on_content_add(&sid, &from, &jingle).await?;
            }
            xmpp_parsers::jingle::Action::ContentAccept => {
                self.on_content_accept(&sid, &from, &jingle).await?;
            }
            xmpp_parsers::jingle::Action::ContentReject => {
                self.on_content_reject(&sid).await;
            }
            xmpp_parsers::jingle::Action::ContentRemove => {
                self.on_content_remove(&sid, &jingle).await;
            }
            xmpp_parsers::jingle::Action::TransportInfo => {
                let media = self.media.read().clone();
                if let Some(media) = media {
                    let xml = element_to_string(Element::from(jingle));
                    let _ = media.add_candidate(&sid, &xml).await;
                }
            }
            xmpp_parsers::jingle::Action::SessionInfo => {}
            _ => {}
        }

        Ok(Some(iq_result(elem)))
    }

    /// Handle a XEP-0353 Jingle Message Initiation message.
    pub async fn handle_jmi_message(self: &Arc<Self>, elem: &Element) -> Result<bool> {
        let Some(child) = elem
            .children()
            .find(|child| child.ns() == NS_JINGLE_MESSAGE)
        else {
            return Ok(false);
        };
        let message = xmpp_parsers::jingle_message::JingleMI::try_from(child.clone())
            .map_err(|error| XmppError::ParseError(error.to_string()))?;
        let from = elem
            .attr("from")
            .and_then(|from| from.parse::<Jid>().ok())
            .ok_or_else(|| XmppError::ProtocolError("JMI message without from".into()))?;
        let bare = from.to_bare();

        match message {
            xmpp_parsers::jingle_message::JingleMI::Propose { sid, description } => {
                self.prune_finished();
                let sid = sid.0;
                if let Some(existing) = self.active_session_for(&bare) {
                    if existing.sid != sid {
                        let busy = xmpp_parsers::jingle_message::JingleMI::Reject(
                            xmpp_parsers::jingle::SessionId(sid.clone()),
                        );
                        self.send_jmi(&from, busy)?;
                        return Ok(true);
                    }
                }
                let kind = CallKind::from_description(&description);
                let session = CallSession {
                    sid: sid.clone(),
                    peer: bare.into(),
                    peer_full: Some(from.clone()),
                    direction: CallDirection::Incoming,
                    kind,
                    camera_enabled: kind == CallKind::Video,
                    camera_pending: false,
                    camera_error: None,
                    screen_sharing: false,
                    screen_pending: false,
                    remote_screen: false,
                    screen_error: None,
                    screen_content: None,
                    state: CallState::Incoming,
                    created_at: Utc::now(),
                    connected_at: None,
                    ended_at: None,
                    reason: None,
                    offer: None,
                    answer: None,
                };
                self.sessions.write().insert(sid.clone(), session);
                let _ = self.events.send(XmppEvent::IncomingCall {
                    sid: sid.clone(),
                    from: from.to_string(),
                    kind: kind.as_str().to_string(),
                });
                self.emit_changed(&sid);
                self.spawn_ring_timeout(sid.clone(), false);
            }
            xmpp_parsers::jingle_message::JingleMI::Accept(sid) => {
                let sid = sid.0;
                if let Some(session) = self.session(&sid) {
                    if session.direction == CallDirection::Outgoing {
                        self.set_peer_full(&sid, &from);
                        self.set_state(&sid, CallState::Ringing, None);
                        let proceed = xmpp_parsers::jingle_message::JingleMI::Proceed(
                            xmpp_parsers::jingle::SessionId(sid.clone()),
                        );
                        self.send_jmi(&bare, proceed)?;
                        self.start_outgoing_media(&sid).await?;
                    }
                }
            }
            xmpp_parsers::jingle_message::JingleMI::Proceed(sid) => {
                let sid = sid.0;
                if let Some(session) = self.session(&sid) {
                    if session.direction == CallDirection::Incoming {
                        self.set_peer_full(&sid, &from);
                    }
                }
            }
            xmpp_parsers::jingle_message::JingleMI::Reject(sid) => {
                let sid = sid.0;
                self.stop_media(&sid);
                self.finish_session(&sid, CallState::Rejected, Some("declined"));
            }
            xmpp_parsers::jingle_message::JingleMI::Retract(sid) => {
                let sid = sid.0;
                self.stop_media(&sid);
                self.finish_session(&sid, CallState::Missed, Some("cancelled"));
            }
        }
        Ok(true)
    }

    async fn on_session_initiate(
        self: &Arc<Self>,
        sid: &str,
        from: &Jid,
        child: Element,
        jingle: &xmpp_parsers::jingle::Jingle,
    ) -> Result<()> {
        let kind = if jingle.contents.iter().any(|content| {
            matches!(
                &content.description,
                Some(xmpp_parsers::jingle::Description::Rtp(rtp)) if rtp.media == "video"
            )
        }) {
            CallKind::Video
        } else {
            CallKind::Audio
        };
        if !self.sessions.read().contains_key(sid) {
            let session = CallSession {
                sid: sid.to_string(),
                peer: from.to_bare().into(),
                peer_full: Some(from.clone()),
                direction: CallDirection::Incoming,
                kind,
                camera_enabled: kind == CallKind::Video,
                camera_pending: false,
                camera_error: None,
                screen_sharing: false,
                screen_pending: false,
                remote_screen: false,
                screen_error: None,
                screen_content: None,
                state: CallState::Connecting,
                created_at: Utc::now(),
                connected_at: None,
                ended_at: None,
                reason: None,
                offer: None,
                answer: None,
            };
            self.sessions.write().insert(sid.to_string(), session);
        }
        self.set_peer_full(sid, from);
        let offer = element_to_string(child);
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.offer = Some(offer.clone());
            session.state = CallState::Connecting;
        }
        self.emit_changed(sid);

        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        let answer = match media.accept_offer(sid, kind, &offer).await {
            Ok(answer) => answer,
            Err(error) => {
                let terminate = build_terminate(sid, "media-error");
                let _ = self.send_jingle_iq(from, &terminate);
                self.stop_media(sid);
                self.finish_session(sid, CallState::Failed, Some(&error.to_string()));
                return Ok(());
            }
        };
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.answer = Some(answer.clone());
        }
        let to = self
            .session(sid)
            .and_then(|session| session.peer_full)
            .unwrap_or_else(|| from.clone());
        let _ = self.send_jingle_iq(&to, &answer);
        self.set_state(sid, CallState::Connecting, None);
        self.mark_active_when_connected(sid.to_string());
        Ok(())
    }

    async fn on_session_accept(
        self: &Arc<Self>,
        sid: &str,
        from: &Jid,
        child: Element,
        _jingle: &xmpp_parsers::jingle::Jingle,
    ) -> Result<()> {
        self.set_peer_full(sid, from);
        let answer = element_to_string(child);
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.answer = Some(answer.clone());
        }
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        media
            .apply_answer(sid, &answer)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))?;
        self.set_state(sid, CallState::Connecting, None);
        self.mark_active_when_connected(sid.to_string());
        Ok(())
    }

    async fn start_outgoing_media(self: &Arc<Self>, sid: &str) -> Result<()> {
        let session = self.require_session(sid)?;
        let media = self
            .media
            .read()
            .clone()
            .ok_or_else(|| XmppError::ProtocolError("no call media backend".into()))?;
        let offer = match media.create_offer(sid, session.kind).await {
            Ok(offer) => offer,
            Err(error) => {
                let to = session.peer_full.clone().unwrap_or(session.peer);
                let retract = xmpp_parsers::jingle_message::JingleMI::Retract(
                    xmpp_parsers::jingle::SessionId(sid.to_string()),
                );
                let _ = self.send_jmi(&to, retract);
                self.stop_media(sid);
                self.finish_session(sid, CallState::Failed, Some(&error.to_string()));
                return Ok(());
            }
        };
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.offer = Some(offer.clone());
            session.state = CallState::Connecting;
        }
        self.emit_changed(sid);
        let to = session
            .peer_full
            .clone()
            .unwrap_or_else(|| session.peer.clone());
        let _ = self.send_jingle_iq(&to, &offer);
        Ok(())
    }

    fn mark_active_when_connected(self: &Arc<Self>, sid: String) {
        let manager = self.clone();
        tokio::spawn(async move {
            for _ in 0..120 {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                let Some(session) = manager.session(&sid) else {
                    return;
                };
                if session.state.is_final() {
                    return;
                }
                if session.state == CallState::Connecting {
                    let media = manager.media.read().clone();
                    if let Some(media) = media {
                        if media.is_connected(&sid) {
                            manager.set_state(&sid, CallState::Active, None);
                            let camera_enabled =
                                if let Some(session) = manager.sessions.write().get_mut(&sid) {
                                    session.connected_at = Some(Utc::now());
                                    session.camera_enabled
                                } else {
                                    false
                                };
                            if camera_enabled {
                                let _ = media.set_camera(&sid, true).await;
                            }
                            manager.emit_changed(&sid);
                            return;
                        }
                    }
                } else if session.state == CallState::Active {
                    return;
                }
            }
        });
    }

    fn prune_finished(&self) {
        let cutoff = Utc::now() - chrono::Duration::minutes(10);
        self.sessions.write().retain(|_, session| {
            !session.state.is_final() || session.ended_at.is_none_or(|ended| ended > cutoff)
        });
    }

    fn spawn_ring_timeout(self: &Arc<Self>, sid: String, outgoing: bool) {
        let manager = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let Some(session) = manager.session(&sid) else {
                return;
            };
            if !matches!(
                session.state,
                CallState::Proposing
                    | CallState::Ringing
                    | CallState::Incoming
                    | CallState::Accepted
            ) {
                return;
            }
            let to = session.peer_full.unwrap_or(session.peer);
            if outgoing {
                let retract = xmpp_parsers::jingle_message::JingleMI::Retract(
                    xmpp_parsers::jingle::SessionId(sid.clone()),
                );
                let _ = manager.send_jmi(&to, retract);
            } else {
                let terminate = build_terminate(&sid, "timeout");
                let _ = manager.send_jingle_iq(&to, &terminate);
            }
            manager.stop_media(&sid);
            manager.finish_session(&sid, CallState::Missed, Some("timeout"));
        });
    }

    fn send_jmi(&self, to: &Jid, payload: xmpp_parsers::jingle_message::JingleMI) -> Result<()> {
        let message = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", format!("jmi-{}", Uuid::new_v4()))
            .append(Element::from(payload))
            .build();
        self.connection.send_stanza_now(message)
    }

    fn send_jingle_iq(&self, to: &Jid, jingle_xml: &str) -> Result<String> {
        let mut payload: Element = jingle_xml
            .parse()
            .map_err(|error: minidom::Error| XmppError::ParseError(error.to_string()))?;
        let own = self.own_jid.read().clone();
        match payload.attr("action") {
            Some("session-initiate") => {
                if let Some(own) = own {
                    payload.set_attr("initiator", own.to_string());
                }
                payload.set_attr("responder", to.to_string());
            }
            Some("session-accept") => {
                if let Some(own) = own {
                    payload.set_attr("responder", own.to_string());
                }
                payload.set_attr("initiator", to.to_string());
            }
            _ => {}
        }
        let id = format!("jingle-{}", Uuid::new_v4());
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", to.to_string())
            .attr("id", id.clone())
            .append(payload)
            .build();
        self.connection.send_stanza_now(iq)?;
        Ok(id)
    }

    fn require_session(&self, sid: &str) -> Result<CallSession> {
        self.session(sid)
            .ok_or_else(|| XmppError::ProtocolError(format!("unknown call session {sid}")))
    }

    fn set_peer_full(&self, sid: &str, from: &Jid) {
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.peer_full = Some(from.clone());
            session.peer = from.to_bare().into();
        }
    }

    fn set_state(&self, sid: &str, state: CallState, reason: Option<&str>) {
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.state = state;
            if let Some(reason) = reason {
                session.reason = Some(reason.to_string());
            }
        }
        self.emit_changed(sid);
    }

    fn finish_session(&self, sid: &str, state: CallState, reason: Option<&str>) {
        tracing::info!("call {sid} finished: {state:?} ({reason:?})");
        if let Some(session) = self.sessions.write().get_mut(sid) {
            session.state = state;
            session.ended_at = Some(Utc::now());
            session.reason = reason.map(str::to_string);
            let reason = session
                .reason
                .clone()
                .unwrap_or_else(|| state.as_str().to_string());
            let _ = self.events.send(XmppEvent::CallEnded {
                sid: sid.to_string(),
                reason,
            });
        }
        self.emit_changed(sid);
    }

    /// Set the playback gain of one conference content.
    pub fn set_content_volume(&self, sid: &str, content: &str, gain: f64) {
        if let Some(media) = self.media.read().as_ref() {
            media.set_content_volume(sid, content, gain);
        }
    }

    /// Whether a conference content carried voice recently.
    #[must_use]
    pub fn content_speaking(&self, sid: &str, content: &str) -> bool {
        self.media
            .read()
            .as_ref()
            .is_some_and(|media| media.content_speaking(sid, content))
    }

    /// Stop the media pipeline of a conference session.
    pub fn stop_session_media(&self, sid: &str) {
        self.stop_media(sid);
    }

    fn stop_media(&self, sid: &str) {
        if let Some(media) = self.media.read().clone() {
            let sid = sid.to_string();
            tokio::spawn(async move {
                let _ = media.stop(&sid).await;
            });
        }
    }

    fn emit_changed(&self, sid: &str) {
        let state = self.session(sid).map_or_else(
            || "ended".to_string(),
            |session| session.state.as_str().to_string(),
        );
        let _ = self.events.send(XmppEvent::CallChanged {
            sid: sid.to_string(),
            state,
        });
    }
}

fn propose_description(kind: CallKind) -> Element {
    Element::builder("description", NS_JINGLE_RTP)
        .attr("media", kind.as_str())
        .append(
            Element::builder("payload-type", NS_JINGLE_RTP)
                .attr("id", "111")
                .attr("name", "opus")
                .attr("clockrate", "48000")
                .attr("channels", "2"),
        )
        .append(Element::builder("rtcp-mux", NS_JINGLE_RTP))
        .build()
}

fn is_screen_content(content: &xmpp_parsers::jingle::Content) -> bool {
    matches!(
        &content.description,
        Some(xmpp_parsers::jingle::Description::Rtp(rtp)) if rtp.media == "video"
    ) && content.disposition == xmpp_parsers::jingle::Disposition::Render
}

fn content_creator(creator: &xmpp_parsers::jingle::Creator) -> &'static str {
    match creator {
        xmpp_parsers::jingle::Creator::Initiator => "initiator",
        xmpp_parsers::jingle::Creator::Responder => "responder",
    }
}

fn content_senders(senders: &xmpp_parsers::jingle::Senders) -> &'static str {
    match senders {
        xmpp_parsers::jingle::Senders::Both => "both",
        xmpp_parsers::jingle::Senders::Initiator => "initiator",
        xmpp_parsers::jingle::Senders::Responder => "responder",
        xmpp_parsers::jingle::Senders::None => "none",
    }
}

fn mark_screen_content(xml: &str, creator: &str, senders: &str) -> String {
    let Ok(mut element) = xml.parse::<Element>() else {
        return xml.to_string();
    };
    for child in element.children_mut() {
        if child.name() == "content" {
            child.set_attr("disposition", "render");
            child.set_attr("creator", creator);
            child.set_attr("senders", senders);
        }
    }
    element_to_string(element)
}

fn content_name(xml: &str) -> Option<String> {
    let element: Element = xml.parse().ok()?;
    let jingle = xmpp_parsers::jingle::Jingle::try_from(element).ok()?;
    jingle
        .contents
        .first()
        .map(|content| content.name.0.clone())
}

fn set_content_creator(xml: &str, creator: &str) -> String {
    let Ok(mut element) = xml.parse::<Element>() else {
        return xml.to_string();
    };
    for child in element.children_mut() {
        if child.name() == "content" {
            child.set_attr("creator", creator);
        }
    }
    element_to_string(element)
}

fn build_content_reject(sid: &str, jingle: &xmpp_parsers::jingle::Jingle) -> String {
    let mut reject = xmpp_parsers::jingle::Jingle::new(
        xmpp_parsers::jingle::Action::ContentReject,
        xmpp_parsers::jingle::SessionId(sid.to_string()),
    );
    reject.contents = jingle.contents.clone();
    element_to_string(Element::from(reject))
}

fn build_terminate(sid: &str, reason: &str) -> String {
    let mut jingle = xmpp_parsers::jingle::Jingle::new(
        xmpp_parsers::jingle::Action::SessionTerminate,
        xmpp_parsers::jingle::SessionId(sid.to_string()),
    );
    let reason_name = match reason {
        "busy" => xmpp_parsers::jingle::Reason::Busy,
        "cancel" => xmpp_parsers::jingle::Reason::Cancel,
        "decline" => xmpp_parsers::jingle::Reason::Decline,
        "timeout" => xmpp_parsers::jingle::Reason::Expired,
        "connectivity-error" => xmpp_parsers::jingle::Reason::ConnectivityError,
        _ => xmpp_parsers::jingle::Reason::Success,
    };
    jingle.reason = Some(xmpp_parsers::jingle::ReasonElement {
        reason: reason_name,
        texts: std::collections::BTreeMap::new(),
    });
    element_to_string(Element::from(jingle))
}

fn element_to_string(element: Element) -> String {
    let mut buffer = Vec::new();
    let _ = element.write_to(&mut buffer);
    String::from_utf8_lossy(&buffer).into_owned()
}

fn iq_result(elem: &Element) -> Element {
    let mut result = Element::builder("iq", ns::JABBER_CLIENT).attr("type", "result");
    if let Some(id) = elem.attr("id") {
        result = result.attr("id", id.to_string());
    }
    if let Some(from) = elem.attr("from") {
        result = result.attr("to", from.to_string());
    }
    result.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_kind_from_description() {
        let description = Element::builder("description", NS_JINGLE_RTP)
            .attr("media", "video")
            .build();
        assert_eq!(CallKind::from_description(&description), CallKind::Video);
    }

    #[test]
    fn terminate_is_valid_jingle() {
        let xml = build_terminate("abc", "success");
        let elem: Element = xml.parse().unwrap();
        let jingle = xmpp_parsers::jingle::Jingle::try_from(elem).unwrap();
        assert_eq!(
            jingle.action,
            xmpp_parsers::jingle::Action::SessionTerminate
        );
        assert_eq!(jingle.sid.0, "abc");
    }

    #[test]
    fn propose_description_has_opus() {
        let description = propose_description(CallKind::Audio);
        assert_eq!(description.attr("media"), Some("audio"));
        assert!(description
            .children()
            .any(|child| child.name() == "payload-type" && child.attr("name") == Some("opus")));
    }

    #[test]
    fn content_reject_echoes_content() {
        let jingle = xmpp_parsers::jingle::Jingle::try_from(
            r#"<jingle xmlns='urn:xmpp:jingle:1' action='content-add' sid='abc'><content creator='initiator' name='video1'/></jingle>"#
                .parse::<Element>()
                .unwrap(),
        )
        .unwrap();
        let xml = build_content_reject("abc", &jingle);
        let reject =
            xmpp_parsers::jingle::Jingle::try_from(xml.parse::<Element>().unwrap()).unwrap();
        assert_eq!(reject.action, xmpp_parsers::jingle::Action::ContentReject);
        assert_eq!(reject.contents.len(), 1);
        assert_eq!(reject.contents[0].name.0, "video1");
    }

    struct FakeMedia {
        events: tokio::sync::broadcast::Sender<MediaEvent>,
        accepted: std::sync::atomic::AtomicUsize,
        applied: std::sync::atomic::AtomicUsize,
        camera_calls: std::sync::Mutex<Vec<bool>>,
        devices: std::sync::Mutex<Vec<CameraDevice>>,
        active_device: std::sync::Mutex<Option<String>>,
        screen_accepted: std::sync::atomic::AtomicUsize,
        screen_applied: std::sync::atomic::AtomicUsize,
        screen_calls: std::sync::Mutex<Vec<bool>>,
    }

    impl FakeMedia {
        fn new() -> Arc<Self> {
            let (events, _) = tokio::sync::broadcast::channel(8);
            Arc::new(Self {
                events,
                accepted: std::sync::atomic::AtomicUsize::new(0),
                applied: std::sync::atomic::AtomicUsize::new(0),
                camera_calls: std::sync::Mutex::new(Vec::new()),
                devices: std::sync::Mutex::new(Vec::new()),
                active_device: std::sync::Mutex::new(None),
                screen_accepted: std::sync::atomic::AtomicUsize::new(0),
                screen_applied: std::sync::atomic::AtomicUsize::new(0),
                screen_calls: std::sync::Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait]
    impl MediaPort for FakeMedia {
        async fn create_offer(&self, _sid: &str, _kind: CallKind) -> anyhow::Result<String> {
            anyhow::bail!("not used")
        }

        async fn accept_offer(
            &self,
            _sid: &str,
            _kind: CallKind,
            _offer: &str,
        ) -> anyhow::Result<String> {
            anyhow::bail!("not used")
        }

        async fn apply_answer(&self, _sid: &str, _answer: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn add_candidate(&self, _sid: &str, _candidate: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn set_muted(&self, _sid: &str, _muted: bool) -> anyhow::Result<()> {
            Ok(())
        }

        async fn set_speaker_muted(&self, _sid: &str, _muted: bool) -> anyhow::Result<()> {
            Ok(())
        }

        async fn set_camera(&self, _sid: &str, enabled: bool) -> anyhow::Result<()> {
            self.camera_calls.lock().unwrap().push(enabled);
            Ok(())
        }

        fn preview_frame(&self, _sid: &str) -> Option<CallPreview> {
            None
        }

        fn remote_preview_frame(&self, _sid: &str) -> Option<CallPreview> {
            None
        }

        fn remote_screen_frame(&self, _sid: &str) -> Option<CallPreview> {
            None
        }

        fn screen_preview_frame(&self, _sid: &str) -> Option<CallPreview> {
            None
        }

        fn clear_remote_screen(&self, _sid: &str) {}

        fn camera_devices(&self) -> Vec<CameraDevice> {
            self.devices.lock().unwrap().clone()
        }

        fn active_camera_device(&self, _sid: &str) -> Option<String> {
            self.active_device.lock().unwrap().clone()
        }

        fn set_preferred_camera(&self, _device_id: Option<&str>) {}

        fn audio_devices(&self) -> Vec<AudioDevice> {
            Vec::new()
        }

        fn set_preferred_audio(&self, _device_id: Option<&str>) {}

        async fn set_camera_device(
            &self,
            _sid: &str,
            device_id: Option<&str>,
        ) -> anyhow::Result<()> {
            *self.active_device.lock().unwrap() = device_id.map(str::to_string);
            Ok(())
        }

        async fn add_video_offer(&self, _sid: &str) -> anyhow::Result<String> {
            anyhow::bail!("not used")
        }

        async fn accept_video_add(&self, _sid: &str, _content_add: &str) -> anyhow::Result<String> {
            self.accepted
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok("<jingle xmlns='urn:xmpp:jingle:1' action='content-accept' sid='sid1'/>".into())
        }

        async fn apply_video_answer(
            &self,
            _sid: &str,
            _content_accept: &str,
        ) -> anyhow::Result<()> {
            self.applied
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }

        async fn add_screen_offer(&self, _sid: &str) -> anyhow::Result<String> {
            anyhow::bail!("not used")
        }

        async fn accept_screen_add(
            &self,
            _sid: &str,
            _content_add: &str,
        ) -> anyhow::Result<String> {
            self.screen_accepted
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok("<jingle xmlns='urn:xmpp:jingle:1' action='content-accept' sid='sid1'/>".into())
        }

        async fn apply_screen_answer(
            &self,
            _sid: &str,
            _content_accept: &str,
        ) -> anyhow::Result<()> {
            self.screen_applied
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }

        async fn set_screen(&self, _sid: &str, enabled: bool) -> anyhow::Result<()> {
            self.screen_calls.lock().unwrap().push(enabled);
            Ok(())
        }

        async fn stop(&self, _sid: &str) -> anyhow::Result<()> {
            Ok(())
        }

        fn is_connected(&self, _sid: &str) -> bool {
            true
        }

        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<MediaEvent> {
            self.events.subscribe()
        }
    }

    fn manager_with_media() -> (Arc<JingleManager>, Arc<FakeMedia>) {
        let (connection, _rx) = Connection::new(crate::ConnectionConfig::default());
        let manager = JingleManager::new(
            Arc::new(connection),
            Arc::new(IqRouter::new(std::time::Duration::from_secs(1))),
            Arc::new(EventBus::new()),
        );
        let media = FakeMedia::new();
        manager.set_media(media.clone());
        (manager, media)
    }

    fn audio_session(direction: CallDirection) -> CallSession {
        CallSession {
            sid: "sid1".to_string(),
            peer: "peer@example.org".parse().unwrap(),
            peer_full: Some("peer@example.org/res".parse().unwrap()),
            direction,
            kind: CallKind::Audio,
            camera_enabled: false,
            camera_pending: false,
            camera_error: None,
            screen_sharing: false,
            screen_pending: false,
            remote_screen: false,
            screen_error: None,
            screen_content: None,
            state: CallState::Active,
            created_at: Utc::now(),
            connected_at: Some(Utc::now()),
            ended_at: None,
            reason: None,
            offer: None,
            answer: None,
        }
    }

    fn jingle_iq(action: &str, content: &str) -> Element {
        format!(
            "<iq xmlns='jabber:client' type='set' from='peer@example.org/res' to='me@example.org/res' id='iq1'><jingle xmlns='urn:xmpp:jingle:1' action='{action}' sid='sid1'>{content}</jingle></iq>"
        )
        .parse()
        .unwrap()
    }

    const VIDEO_CONTENT: &str = "<content creator='initiator' name='video1'><description xmlns='urn:xmpp:jingle:apps:rtp:1' media='video'><payload-type id='97' name='VP8' clockrate='90000'/><rtcp-mux/></description><transport xmlns='urn:xmpp:jingle:transports:ice-udp:1' ufrag='u' pwd='p'/></content>";

    #[tokio::test]
    async fn content_add_upgrades_the_session() {
        let (manager, media) = manager_with_media();
        manager
            .sessions
            .write()
            .insert("sid1".to_string(), audio_session(CallDirection::Incoming));
        manager
            .handle_jingle_iq(&jingle_iq("content-add", VIDEO_CONTENT))
            .await
            .unwrap();
        assert_eq!(media.accepted.load(std::sync::atomic::Ordering::Relaxed), 1);
        let session = manager.session("sid1").unwrap();
        assert_eq!(session.kind, CallKind::Video);
        assert!(!session.camera_enabled);
        assert!(session.camera_error.is_none());
    }

    #[tokio::test]
    async fn content_accept_enables_the_camera() {
        let (manager, media) = manager_with_media();
        let mut session = audio_session(CallDirection::Outgoing);
        session.camera_pending = true;
        manager.sessions.write().insert("sid1".to_string(), session);
        manager
            .handle_jingle_iq(&jingle_iq("content-accept", ""))
            .await
            .unwrap();
        assert_eq!(media.applied.load(std::sync::atomic::Ordering::Relaxed), 1);
        let session = manager.session("sid1").unwrap();
        assert_eq!(session.kind, CallKind::Video);
        assert!(session.camera_enabled);
        assert!(!session.camera_pending);
        assert!(session.camera_error.is_none());
        assert!(media.camera_calls.lock().unwrap().contains(&true));
    }

    const SCREEN_CONTENT: &str = "<content creator='initiator' name='video1' disposition='render'><description xmlns='urn:xmpp:jingle:apps:rtp:1' media='video'><payload-type id='98' name='VP8' clockrate='90000'/><rtcp-mux/></description><transport xmlns='urn:xmpp:jingle:transports:ice-udp:1' ufrag='u' pwd='p'/></content>";

    #[tokio::test]
    async fn screen_add_marks_remote_screen() {
        let (manager, media) = manager_with_media();
        manager
            .sessions
            .write()
            .insert("sid1".to_string(), audio_session(CallDirection::Incoming));
        manager
            .handle_jingle_iq(&jingle_iq("content-add", SCREEN_CONTENT))
            .await
            .unwrap();
        assert_eq!(
            media
                .screen_accepted
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        let session = manager.session("sid1").unwrap();
        assert_eq!(session.kind, CallKind::Video);
        assert!(session.remote_screen);
        assert!(!session.camera_enabled);
    }

    #[tokio::test]
    async fn screen_accept_starts_sharing() {
        let (manager, media) = manager_with_media();
        let mut session = audio_session(CallDirection::Outgoing);
        session.screen_pending = true;
        manager.sessions.write().insert("sid1".to_string(), session);
        manager
            .handle_jingle_iq(&jingle_iq("content-accept", SCREEN_CONTENT))
            .await
            .unwrap();
        assert_eq!(
            media
                .screen_applied
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        let session = manager.session("sid1").unwrap();
        assert_eq!(session.kind, CallKind::Video);
        assert!(session.screen_sharing);
        assert!(!session.screen_pending);
        assert!(media.screen_calls.lock().unwrap().contains(&true));
    }

    #[tokio::test]
    async fn screen_reject_reports_an_error() {
        let (manager, _media) = manager_with_media();
        let mut session = audio_session(CallDirection::Outgoing);
        session.screen_pending = true;
        manager.sessions.write().insert("sid1".to_string(), session);
        manager
            .handle_jingle_iq(&jingle_iq("content-reject", SCREEN_CONTENT))
            .await
            .unwrap();
        let session = manager.session("sid1").unwrap();
        assert!(!session.screen_pending);
        assert!(session.screen_error.is_some());
    }

    #[tokio::test]
    async fn switch_camera_cycles_available_devices() {
        let (manager, media) = manager_with_media();
        *media.devices.lock().unwrap() = vec![
            CameraDevice {
                id: "a".to_string(),
                name: "Camera A".to_string(),
            },
            CameraDevice {
                id: "b".to_string(),
                name: "Camera B".to_string(),
            },
        ];
        manager.switch_camera("sid1").await.unwrap();
        assert_eq!(media.active_device.lock().unwrap().as_deref(), Some("a"));
        manager.switch_camera("sid1").await.unwrap();
        assert_eq!(media.active_device.lock().unwrap().as_deref(), Some("b"));
        manager.switch_camera("sid1").await.unwrap();
        assert_eq!(media.active_device.lock().unwrap().as_deref(), Some("a"));
    }

    #[tokio::test]
    async fn content_reject_reports_an_error() {
        let (manager, _media) = manager_with_media();
        let mut session = audio_session(CallDirection::Outgoing);
        session.camera_pending = true;
        manager.sessions.write().insert("sid1".to_string(), session);
        manager
            .handle_jingle_iq(&jingle_iq("content-reject", ""))
            .await
            .unwrap();
        let session = manager.session("sid1").unwrap();
        assert_eq!(session.kind, CallKind::Audio);
        assert!(!session.camera_pending);
        assert!(session.camera_error.is_some());
    }
}
