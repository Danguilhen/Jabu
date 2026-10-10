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

use dioxus::prelude::{Readable, Signal, Writable};
use jid::Jid;
use minidom::Element;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use xmpp_parsers::ns;

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

impl XmppClient {
    /// Send a 1:1 chat message and record it locally.
    pub fn send_message(&self, to: &Jid, body: &str) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();

        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", id.clone())
            .append(Element::builder("body", ns::JABBER_CLIENT).append(body.to_string()))
            .append(Element::builder("request", "urn:xmpp:receipts"))
            .append(Element::builder(
                "active",
                "http://jabber.org/protocol/chatstates",
            ))
            .build();

        self.connection.send_stanza_now(stanza)?;

        let own_jid = self.connection.jid().unwrap_or_else(|| to.clone());
        let mut msg = ChatMessage::new(
            to.to_bare().to_string(),
            own_jid,
            to.clone(),
            true,
            Some(body.to_string()),
        );
        msg.id = id;

        let mut chat = self.chats.get_or_create(to);
        chat.add_message(msg.clone());

        Ok(msg)
    }

    /// Send a group chat message to a MUC room and record it locally.
    pub fn send_muc_message(&self, room: &Jid, nick: &str, body: &str) -> Result<MucMessage> {
        let id = Uuid::new_v4().to_string();

        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "groupchat")
            .attr("to", room.to_string())
            .attr("id", id.clone())
            .append(Element::builder("body", ns::JABBER_CLIENT).append(body.to_string()))
            .build();

        self.connection.send_stanza_now(stanza)?;

        let mut msg = MucMessage::new(
            room.to_bare().to_string(),
            nick.to_string(),
            self.connection.jid(),
            true,
            Some(body.to_string()),
        );
        msg.id = id;

        let mut room_ref = self.mucs.get_or_create(room);
        room_ref.add_message(msg.clone());

        Ok(msg)
    }

    /// Send a chat state notification (XEP-0085): "composing", "paused", "active", ...
    pub fn send_chat_state(&self, to: &Jid, state: &str) -> Result<()> {
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .append(Element::builder(
                state,
                "http://jabber.org/protocol/chatstates",
            ))
            .build();
        self.connection.send_stanza_now(stanza)
    }

    /// Send a chat marker (XEP-0333): "displayed", "received", "acknowledged".
    pub fn send_marker(&self, to: &Jid, marker: &str, message_id: &str) -> Result<()> {
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .append(
                Element::builder(marker, "urn:xmpp:chat-markers:0")
                    .attr("id", message_id.to_string()),
            )
            .build();
        self.connection.send_stanza_now(stanza)
    }

    /// Send a reply (XEP-0461) to a specific message and record it locally.
    pub fn send_reply(&self, to: &Jid, body: &str, reply_to_id: &str) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", id.clone())
            .append(Element::builder("body", ns::JABBER_CLIENT).append(body.to_string()))
            .append(
                Element::builder("reply", "urn:xmpp:reply:0")
                    .attr("id", reply_to_id.to_string())
                    .attr("to", to.to_bare().to_string()),
            )
            .append(Element::builder("request", "urn:xmpp:receipts"))
            .append(Element::builder(
                "active",
                "http://jabber.org/protocol/chatstates",
            ))
            .build();
        self.connection.send_stanza_now(stanza)?;

        let own = self.connection.jid().unwrap_or_else(|| to.clone());
        let mut msg = ChatMessage::new(
            to.to_bare().to_string(),
            own,
            to.clone(),
            true,
            Some(body.to_string()),
        );
        msg.id = id;
        msg.reply_to = Some(reply_to_id.to_string());

        let mut chat = self.chats.get_or_create(to);
        chat.add_message(msg.clone());
        Ok(msg)
    }

    /// Add reactions to a message (XEP-0444) and update local state.
    pub fn send_reaction(&self, to: &Jid, message_id: &str, emojis: &[&str]) -> Result<()> {
        let mut reactions = Element::builder("reactions", "urn:xmpp:reactions:0")
            .attr("id", message_id.to_string());
        for e in emojis {
            reactions = reactions
                .append(Element::builder("reaction", "urn:xmpp:reactions:0").append(e.to_string()));
        }

        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", format!("react-{}", Uuid::new_v4()))
            .append(reactions)
            .build();
        self.connection.send_stanza_now(stanza)?;

        if let Some(mut chat) = self.chats.get(to) {
            let own = self.connection.jid().unwrap_or_else(|| to.clone());
            let owned: Vec<String> = emojis.iter().map(|s| s.to_string()).collect();
            chat.set_reactions_from(message_id, &own, &owned);
        }
        Ok(())
    }

    /// Correct a previously sent message (XEP-0308) and update local state.
    pub fn send_edit(&self, to: &Jid, message_id: &str, new_body: &str) -> Result<()> {
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", format!("edit-{}", Uuid::new_v4()))
            .append(Element::builder("body", ns::JABBER_CLIENT).append(new_body.to_string()))
            .append(
                Element::builder("replace", "urn:xmpp:message-correct:0")
                    .attr("id", message_id.to_string()),
            )
            .build();
        self.connection.send_stanza_now(stanza)?;

        if let Some(mut chat) = self.chats.get(to) {
            chat.apply_edit(message_id, new_body);
        }
        Ok(())
    }

    /// Retract a previously sent message (XEP-0424) and update local state.
    pub fn send_retract(&self, to: &Jid, message_id: &str) -> Result<()> {
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", format!("retract-{}", Uuid::new_v4()))
            .append(
                Element::builder("retract", "urn:xmpp:message-retract:0")
                    .attr("id", message_id.to_string()),
            )
            .build();
        self.connection.send_stanza_now(stanza)?;

        if let Some(mut chat) = self.chats.get(to) {
            chat.retract(message_id);
        }
        Ok(())
    }

    /// Join a MUC room (XEP-0045) by sending presence to `room@service/nick`.
    pub fn join_room(&self, room: &Jid, nick: &str, password: Option<&str>) -> Result<MucRoom> {
        let mut muc = Element::builder("x", "http://jabber.org/protocol/muc");
        if let Some(pw) = password {
            muc = muc.append(
                Element::builder("password", "http://jabber.org/protocol/muc")
                    .append(pw.to_string()),
            );
        }
        let presence = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("to", format!("{}/{}", room.to_bare(), nick))
            .append(muc)
            .build();
        self.connection.send_stanza_now(presence)?;

        let mut room_ref = self.mucs.get_or_create(room);
        room_ref.join(nick, password.map(|p| p.to_string()))?;
        Ok(room_ref)
    }

    /// Leave a MUC room.
    pub fn leave_room(&self, room: &Jid) -> Result<()> {
        let presence = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("to", format!("{}/{}", room.to_bare(), "me"))
            .attr("type", "unavailable")
            .build();
        self.connection.send_stanza_now(presence)?;
        if let Some(mut r) = self.mucs.get_by_bare(&room.to_bare().to_string()) {
            r.leave(None)?;
        }
        Ok(())
    }

    /// Add a contact to the roster and request presence subscription.
    pub fn add_contact(&self, jid: &Jid, name: Option<&str>) -> Result<()> {
        let mut item =
            Element::builder("item", "jabber:iq:roster").attr("jid", jid.to_bare().to_string());
        if let Some(n) = name {
            item = item.attr("name", n.to_string());
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("id", format!("roster-set-{}", Uuid::new_v4()))
            .append(Element::builder("query", "jabber:iq:roster").append(item))
            .build();
        self.connection.send_stanza_now(iq)?;

        let subscribe = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("type", "subscribe")
            .attr("to", jid.to_bare().to_string())
            .build();
        self.connection.send_stanza_now(subscribe)?;
        Ok(())
    }

    /// Add a contact by its string JID (convenience wrapper for the UI).
    pub fn add_contact_str(&self, jid: &str, name: Option<&str>) -> Result<()> {
        let jid: Jid = jid
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.add_contact(&jid, name)
    }

    /// Join a room by its string JID (convenience wrapper for the UI).
    pub fn join_room_str(&self, room: &str, nick: &str) -> Result<MucRoom> {
        let jid: Jid = room
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.join_room(&jid, nick, None)
    }

    /// Query the archive for a conversation given as string (convenience wrapper).
    pub fn query_mam_str(&self, with: &str, max: u32) -> Result<()> {
        let jid: Jid = with
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.query_mam(&jid, max)
    }

    /// Query the message archive (XEP-0313, `urn:xmpp:mam:2`) for a conversation.
    pub fn query_mam(&self, with: &Jid, max: u32) -> Result<()> {
        let query_id = format!("q-{}", Uuid::new_v4());
        let fields = Element::builder("x", "jabber:x:data")
            .attr("type", "submit")
            .append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", "FORM_TYPE")
                    .attr("type", "hidden")
                    .append(Element::builder("value", "jabber:x:data").append("urn:xmpp:mam:2")),
            )
            .append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", "with")
                    .append(
                        Element::builder("value", "jabber:x:data")
                            .append(with.to_bare().to_string()),
                    ),
            );

        let query = Element::builder("query", "urn:xmpp:mam:2")
            .attr("queryid", query_id)
            .append(fields)
            .append(
                Element::builder("set", "http://jabber.org/protocol/rsm").append(
                    Element::builder("max", "http://jabber.org/protocol/rsm")
                        .append(max.to_string()),
                ),
            );

        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("id", format!("mam-{}", Uuid::new_v4()))
            .append(query)
            .build();
        self.connection.send_stanza_now(iq)
    }
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
    /// Bumped whenever the underlying XMPP data changes, so components re-read it.
    pub revision: Signal<u64>,
    /// Bare JID of the currently open conversation (1:1 chat or MUC room).
    pub selected_chat: Signal<Option<String>>,
}

impl ClientState {
    pub fn new() -> Self {
        let config = Signal::new(None);
        let client = Signal::new(None);
        let connection_state = Signal::new(ConnectionState::Disconnected);
        let is_connected = Signal::new(false);
        let is_authenticated = Signal::new(false);
        let last_error = Signal::new(None);
        let revision = Signal::new(0);
        let selected_chat = Signal::new(None);

        Self {
            config,
            client,
            connection_state,
            is_connected,
            is_authenticated,
            last_error,
            revision,
            selected_chat,
        }
    }

    /// Signal the UI that underlying data changed.
    pub fn bump(&mut self) {
        let next = *self.revision.read() + 1;
        self.revision.set(next);
    }

    /// Select a conversation by bare JID (or clear it).
    pub fn select(&mut self, jid: Option<String>) {
        self.selected_chat.set(jid);
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
