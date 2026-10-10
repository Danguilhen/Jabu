//! XMPP Core - Direct tokio-xmpp implementation for Jabu

pub mod chat;
pub mod connection;
pub mod error;
pub mod events;
pub mod muc;
pub mod processor;
pub mod pubsub;
pub mod roster;
pub mod storage;
pub mod types;

pub use chat::{Chat, ChatManager, ChatMessage, ChatState, MessageReceipt, MessageType};
pub use connection::{Connection, ConnectionConfig, ConnectionEvent, ConnectionState};
pub use error::{Result, XmppError};
pub use events::{EventBus, XmppEvent};
pub use muc::{MucAffiliation, MucConfig, MucManager, MucMessage, MucOccupant, MucRole, MucRoom};
pub use processor::EventProcessor;
pub use pubsub::{PubSubItem, PubSubManager, PubSubNode, PubSubService, PubSubSubscription};
pub use roster::{Presence, Roster, RosterGroup, RosterItem, SubscriptionState};
pub use types::{EncryptionInfo, MediaAttachment, MediaType, Reaction};

use dioxus::prelude::Signal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Main XMPP client context that holds all state
#[derive(Clone)]
pub struct XmppClient {
    pub connection: Arc<Connection>,
    pub roster: Arc<Roster>,
    pub chats: Arc<ChatManager>,
    pub mucs: Arc<MucManager>,
    pub pubsub: Arc<PubSubManager>,
    pub events: Arc<EventBus>,
}

/// Configuration for the XMPP client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub jid: String,
    pub password: Option<String>,
    pub resource: Option<String>,
    pub priority: i32,
    pub auto_reconnect: bool,
    pub reconnect_interval: u64,
    pub max_reconnect_attempts: u32,
    pub enable_stream_management: bool,
    pub enable_compression: bool,
    pub keepalive_interval: u64,
    pub tls_config: TlsConfig,
    pub proxy_config: Option<ProxyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub verify_hostname: bool,
    pub verify_cert: bool,
    pub ca_file: Option<String>,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    Socks5,
    Http,
    Https,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            jid: String::new(),
            password: None,
            resource: Some("jabu".to_string()),
            priority: 0,
            auto_reconnect: true,
            reconnect_interval: 5,
            max_reconnect_attempts: 10,
            enable_stream_management: true,
            enable_compression: false,
            keepalive_interval: 60,
            tls_config: TlsConfig::default(),
            proxy_config: None,
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            verify_hostname: true,
            verify_cert: true,
            ca_file: None,
            cert_file: None,
            key_file: None,
        }
    }
}

/// Global client state for Dioxus signals
#[derive(Clone)]
pub struct ClientState {
    pub config: Signal<Option<ClientConfig>>,
    pub client: Signal<Option<XmppClient>>,
    pub connection_state: Signal<ConnectionState>,
    pub is_connected: Signal<bool>,
    pub is_authenticated: Signal<bool>,
    pub last_error: Signal<Option<String>>,
}

impl ClientState {
    pub fn new() -> Self {
        let config = Signal::new(None);
        let client = Signal::new(None);
        let connection_state = Signal::new(ConnectionState::Disconnected);
        let is_connected = Signal::new(false);
        let is_authenticated = Signal::new(false);
        let last_error = Signal::new(None);

        Self {
            config,
            client,
            connection_state,
            is_connected,
            is_authenticated,
            last_error,
        }
    }
}

impl Default for ClientState {
    fn default() -> Self {
        Self::new()
    }
}

impl From<ClientConfig> for ConnectionConfig {
    fn from(config: ClientConfig) -> Self {
        Self {
            jid: config.jid,
            password: config.password,
            resource: config.resource,
            priority: config.priority,
            auto_reconnect: config.auto_reconnect,
            reconnect_interval: config.reconnect_interval,
            max_reconnect_attempts: config.max_reconnect_attempts,
            enable_stream_management: config.enable_stream_management,
            enable_compression: config.enable_compression,
            keepalive_interval: config.keepalive_interval,
            tls_config: config.tls_config,
            proxy_config: config.proxy_config,
        }
    }
}
