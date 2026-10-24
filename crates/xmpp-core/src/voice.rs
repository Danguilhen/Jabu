//! Voice channels for spaces.
//!
//! Voice channels are Discord-style rooms that connect to an SFU instead of
//! using a full-mesh of one-to-one Jingle calls, so a channel can hold more
//! than a handful of participants.

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::{Result, XmppError};

/// One voice channel inside a space.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceChannel {
    pub space_server: String,
    pub space_node: String,
    pub room: String,
    pub name: String,
}

/// A participant currently connected to a voice channel.
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceParticipant {
    pub id: String,
    pub name: String,
    pub muted: bool,
    pub speaking: bool,
    pub screen_sharing: bool,
    pub is_self: bool,
    /// Playback gain for this participant, where 1.0 is 100%.
    pub volume: f64,
}

/// Everything needed to connect to one voice channel.
#[derive(Clone, Debug)]
pub struct VoiceJoin {
    pub channel: VoiceChannel,
    pub username: String,
    pub microphone: Option<String>,
    pub muted: bool,
}

/// Events emitted while a voice channel is connected.
#[derive(Clone, Debug)]
pub enum VoiceEvent {
    Joined,
    Participants(Vec<VoiceParticipant>),
    Left,
    Error(String),
}

/// Media backend for voice channels (an SFU client).
#[async_trait]
pub trait VoicePort: Send + Sync {
    async fn join(&self, request: VoiceJoin) -> anyhow::Result<()>;
    async fn leave(&self) -> anyhow::Result<()>;
    async fn set_muted(&self, muted: bool) -> anyhow::Result<()>;
    async fn set_participant_volume(&self, id: &str, volume: f64) -> anyhow::Result<()>;
    fn is_joined(&self) -> bool;
    fn channel(&self) -> Option<VoiceChannel>;
    fn participants(&self) -> Vec<VoiceParticipant>;
    fn subscribe(&self) -> broadcast::Receiver<VoiceEvent>;
}

/// Holds the installed voice backend and forwards calls to it.
#[derive(Default)]
pub struct VoiceManager {
    port: RwLock<Option<Arc<dyn VoicePort>>>,
}

impl VoiceManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_port(&self, port: Arc<dyn VoicePort>) {
        *self.port.write() = Some(port);
    }

    #[must_use]
    pub fn port(&self) -> Option<Arc<dyn VoicePort>> {
        self.port.read().clone()
    }

    /// Connect to a voice channel.
    ///
    /// # Errors
    ///
    /// Returns an error when no voice backend is installed or the SFU rejects
    /// the connection.
    pub async fn join(&self, request: VoiceJoin) -> Result<()> {
        let port = self
            .port()
            .ok_or_else(|| XmppError::ProtocolError("no voice backend".into()))?;
        port.join(request)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))
    }

    /// Disconnect from the current voice channel.
    ///
    /// # Errors
    ///
    /// Returns an error when no voice backend is installed.
    pub async fn leave(&self) -> Result<()> {
        let port = self
            .port()
            .ok_or_else(|| XmppError::ProtocolError("no voice backend".into()))?;
        port.leave()
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))
    }

    /// Mute or unmute the microphone in the current voice channel.
    ///
    /// # Errors
    ///
    /// Returns an error when no voice backend is installed.
    pub async fn set_muted(&self, muted: bool) -> Result<()> {
        let port = self
            .port()
            .ok_or_else(|| XmppError::ProtocolError("no voice backend".into()))?;
        port.set_muted(muted)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))
    }

    /// Set the playback gain of one participant (1.0 is 100%).
    ///
    /// # Errors
    ///
    /// Returns an error when no voice backend is installed.
    pub async fn set_participant_volume(&self, id: &str, volume: f64) -> Result<()> {
        let port = self
            .port()
            .ok_or_else(|| XmppError::ProtocolError("no voice backend".into()))?;
        port.set_participant_volume(id, volume)
            .await
            .map_err(|error| XmppError::ProtocolError(error.to_string()))
    }

    #[must_use]
    pub fn is_joined(&self) -> bool {
        self.port().is_some_and(|port| port.is_joined())
    }

    #[must_use]
    pub fn channel(&self) -> Option<VoiceChannel> {
        self.port().and_then(|port| port.channel())
    }

    #[must_use]
    pub fn participants(&self) -> Vec<VoiceParticipant> {
        self.port()
            .map(|port| port.participants())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn subscribe(&self) -> Option<broadcast::Receiver<VoiceEvent>> {
        self.port().map(|port| port.subscribe())
    }
}
