//! Event bus for XMPP events

use jid::Jid;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::Result;

/// Global event bus for XMPP events
pub struct EventBus {
    tx: broadcast::Sender<XmppEvent>,
    subscribers: RwLock<HashMap<String, Vec<Subscriber>>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1000);
        Self {
            tx,
            subscribers: RwLock::new(HashMap::new()),
        }
    }

    pub fn send(&self, event: XmppEvent) -> Result<()> {
        let _ = self.tx.send(event);
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<XmppEvent> {
        self.tx.subscribe()
    }

    pub fn subscribe_filtered<F>(&self, filter: F) -> FilteredReceiver
    where
        F: Fn(&XmppEvent) -> bool + Send + Sync + 'static,
    {
        let rx = self.tx.subscribe();
        FilteredReceiver {
            rx,
            filter: Box::new(filter),
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Filtered event receiver
pub struct FilteredReceiver {
    rx: broadcast::Receiver<XmppEvent>,
    filter: Box<dyn Fn(&XmppEvent) -> bool + Send + Sync>,
}

impl FilteredReceiver {
    pub async fn recv(&mut self) -> std::result::Result<XmppEvent, broadcast::error::RecvError> {
        loop {
            let event = self.rx.recv().await?;
            if (self.filter)(&event) {
                return Ok(event);
            }
        }
    }
}

/// Subscriber for typed events
struct Subscriber {
    id: String,
    callback: Box<dyn Fn(&XmppEvent) + Send + Sync>,
}

/// XMPP Events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum XmppEvent {
    // Connection events
    Connected {
        jid: String,
    },
    Authenticated,
    Disconnected {
        reason: String,
    },
    Reconnecting {
        attempt: u32,
    },
    ReconnectTriggered,
    ReconnectFailed,
    StanzaReceived(String),
    SendStanza(String),

    // Roster events
    RosterChanged,
    ContactAdded {
        jid: String,
    },
    ContactRemoved {
        jid: String,
    },
    ContactUpdated {
        jid: String,
    },
    PresenceChanged {
        jid: String,
        presence: crate::roster::Presence,
    },
    SubscriptionRequest {
        from: String,
    },
    SubscriptionApproved {
        jid: String,
    },
    SubscriptionDenied {
        jid: String,
    },

    // Chat events
    ChatChanged {
        jid: String,
    },
    MessageReceived {
        jid: String,
        message: crate::chat::ChatMessage,
    },
    MessageSent {
        jid: String,
        message: crate::chat::ChatMessage,
    },
    MessageUpdated {
        jid: String,
        message_id: String,
    },
    MessageDeleted {
        jid: String,
        message_id: String,
    },
    MessageReceipt {
        jid: String,
        message_id: String,
        receipt: crate::chat::MessageReceipt,
    },
    ChatStateChanged {
        jid: String,
        state: crate::chat::ChatStateNotification,
    },
    ReactionAdded {
        jid: String,
        message_id: String,
        reaction: crate::chat::Reaction,
    },
    ReactionRemoved {
        jid: String,
        message_id: String,
        reaction: crate::chat::Reaction,
    },

    // MUC events
    MucChanged {
        jid: String,
    },
    MucJoined {
        jid: String,
        nick: String,
    },
    MucLeft {
        jid: String,
        reason: Option<String>,
    },
    MucOccupantJoined {
        jid: String,
        occupant: crate::muc::MucOccupant,
    },
    MucOccupantLeft {
        jid: String,
        nick: String,
    },
    MucOccupantUpdated {
        jid: String,
        occupant: crate::muc::MucOccupant,
    },
    MucMessageReceived {
        jid: String,
        message: crate::muc::MucMessage,
    },
    MucSubjectChanged {
        jid: String,
        subject: String,
        by: Option<String>,
    },
    MucConfigChanged {
        jid: String,
    },
    MucInvitationReceived {
        jid: String,
        from: String,
        reason: Option<String>,
    },

    // PubSub events
    PubSubNodeChanged {
        service_jid: String,
        node_id: String,
    },
    PubSubItemPublished {
        service_jid: String,
        node_id: String,
        item: crate::pubsub::PubSubItem,
    },
    PubSubItemRetracted {
        service_jid: String,
        node_id: String,
        item_id: String,
    },
    PubSubItemDeleted {
        service_jid: String,
        node_id: String,
        item_id: String,
    },
    PubSubSubscriptionChanged {
        service_jid: String,
        node_id: String,
        jid: String,
        subscribed: bool,
    },

    // Error events
    Error {
        message: String,
    },
    Warning {
        message: String,
    },
}

/// Event filter helpers
impl XmppEvent {
    pub fn is_connection_event(&self) -> bool {
        matches!(
            self,
            XmppEvent::Connected { .. }
                | XmppEvent::Authenticated
                | XmppEvent::Disconnected { .. }
                | XmppEvent::Reconnecting { .. }
                | XmppEvent::ReconnectTriggered
                | XmppEvent::ReconnectFailed
                | XmppEvent::StanzaReceived(_)
                | XmppEvent::SendStanza(_)
        )
    }

    pub fn is_roster_event(&self) -> bool {
        matches!(
            self,
            XmppEvent::RosterChanged
                | XmppEvent::ContactAdded { .. }
                | XmppEvent::ContactRemoved { .. }
                | XmppEvent::ContactUpdated { .. }
                | XmppEvent::PresenceChanged { .. }
                | XmppEvent::SubscriptionRequest { .. }
                | XmppEvent::SubscriptionApproved { .. }
                | XmppEvent::SubscriptionDenied { .. }
        )
    }

    pub fn is_chat_event(&self) -> bool {
        matches!(
            self,
            XmppEvent::ChatChanged { .. }
                | XmppEvent::MessageReceived { .. }
                | XmppEvent::MessageSent { .. }
                | XmppEvent::MessageUpdated { .. }
                | XmppEvent::MessageDeleted { .. }
                | XmppEvent::MessageReceipt { .. }
                | XmppEvent::ChatStateChanged { .. }
                | XmppEvent::ReactionAdded { .. }
                | XmppEvent::ReactionRemoved { .. }
        )
    }

    pub fn is_muc_event(&self) -> bool {
        matches!(
            self,
            XmppEvent::MucChanged { .. }
                | XmppEvent::MucJoined { .. }
                | XmppEvent::MucLeft { .. }
                | XmppEvent::MucOccupantJoined { .. }
                | XmppEvent::MucOccupantLeft { .. }
                | XmppEvent::MucOccupantUpdated { .. }
                | XmppEvent::MucMessageReceived { .. }
                | XmppEvent::MucSubjectChanged { .. }
                | XmppEvent::MucConfigChanged { .. }
                | XmppEvent::MucInvitationReceived { .. }
        )
    }

    pub fn is_pubsub_event(&self) -> bool {
        matches!(
            self,
            XmppEvent::PubSubNodeChanged { .. }
                | XmppEvent::PubSubItemPublished { .. }
                | XmppEvent::PubSubItemRetracted { .. }
                | XmppEvent::PubSubItemDeleted { .. }
                | XmppEvent::PubSubSubscriptionChanged { .. }
        )
    }

    pub fn bare_jid(&self) -> Option<String> {
        match self {
            XmppEvent::Connected { jid } => Some(jid.clone()),
            XmppEvent::ContactAdded { jid } => Some(jid.clone()),
            XmppEvent::ContactRemoved { jid } => Some(jid.clone()),
            XmppEvent::ContactUpdated { jid } => Some(jid.clone()),
            XmppEvent::PresenceChanged { jid, .. } => Some(jid.clone()),
            XmppEvent::SubscriptionRequest { from } => Some(from.clone()),
            XmppEvent::SubscriptionApproved { jid } => Some(jid.clone()),
            XmppEvent::SubscriptionDenied { jid } => Some(jid.clone()),
            XmppEvent::ChatChanged { jid } => Some(jid.clone()),
            XmppEvent::MessageReceived { jid, .. } => Some(jid.clone()),
            XmppEvent::MessageSent { jid, .. } => Some(jid.clone()),
            XmppEvent::MessageUpdated { jid, .. } => Some(jid.clone()),
            XmppEvent::MessageDeleted { jid, .. } => Some(jid.clone()),
            XmppEvent::MessageReceipt { jid, .. } => Some(jid.clone()),
            XmppEvent::ChatStateChanged { jid, .. } => Some(jid.clone()),
            XmppEvent::ReactionAdded { jid, .. } => Some(jid.clone()),
            XmppEvent::ReactionRemoved { jid, .. } => Some(jid.clone()),
            XmppEvent::MucChanged { jid } => Some(jid.clone()),
            XmppEvent::MucJoined { jid, .. } => Some(jid.clone()),
            XmppEvent::MucLeft { jid, .. } => Some(jid.clone()),
            XmppEvent::MucOccupantJoined { jid, .. } => Some(jid.clone()),
            XmppEvent::MucOccupantLeft { jid, .. } => Some(jid.clone()),
            XmppEvent::MucOccupantUpdated { jid, .. } => Some(jid.clone()),
            XmppEvent::MucMessageReceived { jid, .. } => Some(jid.clone()),
            XmppEvent::MucSubjectChanged { jid, .. } => Some(jid.clone()),
            XmppEvent::MucConfigChanged { jid } => Some(jid.clone()),
            XmppEvent::MucInvitationReceived { jid, .. } => Some(jid.clone()),
            XmppEvent::PubSubItemPublished { service_jid, .. } => Some(service_jid.clone()),
            XmppEvent::PubSubItemRetracted { service_jid, .. } => Some(service_jid.clone()),
            XmppEvent::PubSubItemDeleted { service_jid, .. } => Some(service_jid.clone()),
            XmppEvent::PubSubSubscriptionChanged { service_jid, .. } => Some(service_jid.clone()),
            _ => None,
        }
    }
}

/// Event statistics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct EventStats {
    pub total_events: u64,
    pub connection_events: u64,
    pub roster_events: u64,
    pub chat_events: u64,
    pub muc_events: u64,
    pub pubsub_events: u64,
    pub error_events: u64,
}
