//! Error types for XMPP core

use jid;
use std::fmt;
use thiserror::Error;

/// Result type for XMPP operations
pub type Result<T> = std::result::Result<T, XmppError>;

/// XMPP Error types
#[derive(Debug, Error)]
pub enum XmppError {
    #[error("Invalid JID: {0}")]
    InvalidJid(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Not connected to XMPP server")]
    NotConnected,

    #[error("Already connected")]
    AlreadyConnected,

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Disconnect failed: {0}")]
    DisconnectFailed(String),

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Receive failed: {0}")]
    ReceiveFailed(String),

    #[error("Stream error: {0}")]
    StreamError(String),

    #[error("TLS error: {0}")]
    TlsError(String),

    #[error("SASL error: {0}")]
    SaslError(String),

    #[error("Roster error: {0}")]
    RosterError(String),

    #[error("Chat error: {0}")]
    ChatError(String),

    #[error("MUC error: {0}")]
    MucError(String),

    #[error("PubSub error: {0}")]
    PubSubError(String),

    #[error("Upload error: {0}")]
    UploadError(String),

    #[error("Cryptography error: {0}")]
    CryptoError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Migration error: {0}")]
    MigrationError(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Feature not supported: {0}")]
    FeatureNotSupported(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("UTF-8 error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("JID error: {0}")]
    JidError(#[from] jid::Error),

    #[error("UUID error: {0}")]
    UuidError(#[from] uuid::Error),

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("Other error: {0}")]
    Other(String),
}

impl XmppError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            XmppError::ConnectionFailed(_)
                | XmppError::NetworkError(_)
                | XmppError::Timeout(_)
                | XmppError::RateLimited(_)
                | XmppError::TlsError(_)
                | XmppError::ChannelError(_)
        )
    }

    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            XmppError::AuthenticationFailed(_)
                | XmppError::PermissionDenied(_)
                | XmppError::ConfigurationError(_)
                | XmppError::InvalidJid(_)
        )
    }

    pub fn error_code(&self) -> Option<&'static str> {
        match self {
            XmppError::InvalidJid(_) => Some("invalid-jid"),
            XmppError::ConnectionFailed(_) => Some("connection-failed"),
            XmppError::NotConnected => Some("not-connected"),
            XmppError::AlreadyConnected => Some("already-connected"),
            XmppError::AuthenticationFailed(_) => Some("authentication-failed"),
            XmppError::DisconnectFailed(_) => Some("disconnect-failed"),
            XmppError::SendFailed(_) => Some("send-failed"),
            XmppError::ReceiveFailed(_) => Some("receive-failed"),
            XmppError::StreamError(_) => Some("stream-error"),
            XmppError::TlsError(_) => Some("tls-error"),
            XmppError::SaslError(_) => Some("sasl-error"),
            XmppError::RosterError(_) => Some("roster-error"),
            XmppError::ChatError(_) => Some("chat-error"),
            XmppError::MucError(_) => Some("muc-error"),
            XmppError::PubSubError(_) => Some("pubsub-error"),
            XmppError::UploadError(_) => Some("upload-error"),
            XmppError::CryptoError(_) => Some("crypto-error"),
            XmppError::StorageError(_) => Some("storage-error"),
            XmppError::SerializationError(_) => Some("serialization-error"),
            XmppError::DeserializationError(_) => Some("deserialization-error"),
            XmppError::DatabaseError(_) => Some("database-error"),
            XmppError::MigrationError(_) => Some("migration-error"),
            XmppError::ConfigurationError(_) => Some("configuration-error"),
            XmppError::InvalidState(_) => Some("invalid-state"),
            XmppError::Timeout(_) => Some("timeout"),
            XmppError::RateLimited(_) => Some("rate-limited"),
            XmppError::NotFound(_) => Some("not-found"),
            XmppError::AlreadyExists(_) => Some("already-exists"),
            XmppError::PermissionDenied(_) => Some("permission-denied"),
            XmppError::FeatureNotSupported(_) => Some("feature-not-supported"),
            XmppError::ProtocolError(_) => Some("protocol-error"),
            XmppError::NetworkError(_) => Some("network-error"),
            XmppError::IoError(_) => Some("io-error"),
            XmppError::ParseError(_) => Some("parse-error"),
            XmppError::Utf8Error(_) => Some("utf8-error"),
            XmppError::JsonError(_) => Some("json-error"),
            XmppError::JidError(_) => Some("jid-error"),
            XmppError::UuidError(_) => Some("uuid-error"),
            XmppError::ChannelError(_) => Some("channel-error"),
            XmppError::Other(_) => Some("other"),
        }
    }
}

impl From<redb::Error> for XmppError {
    fn from(err: redb::Error) -> Self {
        XmppError::DatabaseError(err.to_string())
    }
}

impl From<redb::TransactionError> for XmppError {
    fn from(err: redb::TransactionError) -> Self {
        XmppError::DatabaseError(err.to_string())
    }
}

impl From<redb::TableError> for XmppError {
    fn from(err: redb::TableError) -> Self {
        XmppError::DatabaseError(err.to_string())
    }
}

impl From<redb::StorageError> for XmppError {
    fn from(err: redb::StorageError) -> Self {
        XmppError::DatabaseError(err.to_string())
    }
}

impl From<redb::CommitError> for XmppError {
    fn from(err: redb::CommitError) -> Self {
        XmppError::DatabaseError(err.to_string())
    }
}

impl From<rusqlite::Error> for XmppError {
    fn from(err: rusqlite::Error) -> Self {
        XmppError::DatabaseError(err.to_string())
    }
}

impl From<tokio::task::JoinError> for XmppError {
    fn from(err: tokio::task::JoinError) -> Self {
        XmppError::Other(err.to_string())
    }
}

impl From<tokio::sync::broadcast::error::SendError<crate::events::XmppEvent>> for XmppError {
    fn from(err: tokio::sync::broadcast::error::SendError<crate::events::XmppEvent>) -> Self {
        XmppError::ChannelError(err.to_string())
    }
}

impl From<tokio::sync::broadcast::error::RecvError> for XmppError {
    fn from(err: tokio::sync::broadcast::error::RecvError) -> Self {
        XmppError::ChannelError(err.to_string())
    }
}
