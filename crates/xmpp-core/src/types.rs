//! Shared types used across chat, MUC, and PubSub modules

use chrono::Utc;
use jid::Jid;
use serde::{Deserialize, Serialize};

/// Reaction to a message (XEP-0444)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    pub from_jid: Jid,
    pub timestamp: chrono::DateTime<Utc>,
}

/// Encryption information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionInfo {
    pub namespace: String,
    pub name: String,
    pub fingerprint: Option<String>,
}

/// Media attachment
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaAttachment {
    pub id: String,
    pub url: String,
    pub media_type: MediaType,
    #[serde(default)]
    pub mime: Option<String>,
    pub name: Option<String>,
    pub size: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration: Option<f64>,
    pub thumbnail_url: Option<String>,
    pub blur_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Image,
    Video,
    Audio,
    File,
    Location,
    Contact,
}

impl MediaType {
    #[must_use]
    pub fn from_mime(mime: &str) -> Self {
        if mime.starts_with("image/") {
            Self::Image
        } else if mime.starts_with("video/") {
            Self::Video
        } else if mime.starts_with("audio/") {
            Self::Audio
        } else {
            Self::File
        }
    }
}
