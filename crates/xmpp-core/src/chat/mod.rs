//! 1:1 Chat management

use dioxus::prelude::{Readable, Signal, Writable};
use jid::Jid;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

use crate::{EventBus, Result, XmppEvent};

/// Chat manager for 1:1 conversations
pub struct ChatManager {
    chats: RwLock<HashMap<String, Chat>>,
    events: Arc<EventBus>,
}

impl ChatManager {
    pub fn new(events: Arc<EventBus>) -> Self {
        Self {
            chats: RwLock::new(HashMap::new()),
            events,
        }
    }

    pub fn get_or_create(&self, jid: &Jid) -> Chat {
        let bare_jid = jid.to_bare().to_string();
        let mut chats = self.chats.write();
        chats
            .entry(bare_jid.clone())
            .or_insert_with(|| Chat::new(bare_jid, self.events.clone()))
            .clone()
    }

    pub fn get(&self, jid: &Jid) -> Option<Chat> {
        let bare_jid = jid.to_bare().to_string();
        self.chats.read().get(&bare_jid).cloned()
    }

    pub fn remove(&self, jid: &Jid) -> Option<Chat> {
        let bare_jid = jid.to_bare().to_string();
        self.chats.write().remove(&bare_jid)
    }

    pub fn list(&self) -> Vec<Chat> {
        self.chats.read().values().cloned().collect()
    }

    pub fn list_with_unread(&self) -> Vec<Chat> {
        self.chats
            .read()
            .values()
            .filter(|c| c.unread_count() > 0)
            .cloned()
            .collect()
    }

    /// Look up a chat by its bare JID string (as stored in the map).
    pub fn get_by_bare(&self, bare: &str) -> Option<Chat> {
        self.chats.read().get(bare).cloned()
    }
}

/// Individual chat session
#[derive(Clone)]
pub struct Chat {
    pub jid: Jid,
    pub bare_jid: String,
    messages: Arc<RwLock<Vec<ChatMessage>>>,
    mam_cursor: Arc<RwLock<Option<(chrono::DateTime<chrono::Utc>, String)>>>,
    unread_count: Arc<RwLock<usize>>,
    state: Arc<RwLock<ChatState>>,
    peer_state: Arc<RwLock<ChatStateNotification>>,
    events: Arc<EventBus>,
    changed: Signal<u64>,
}

impl Chat {
    pub fn new(bare_jid: String, events: Arc<EventBus>) -> Self {
        let jid = bare_jid.parse().unwrap();
        Self {
            jid,
            bare_jid: bare_jid.clone(),
            messages: Arc::new(RwLock::new(Vec::new())),
            mam_cursor: Arc::new(RwLock::new(None)),
            unread_count: Arc::new(RwLock::new(0)),
            state: Arc::new(RwLock::new(ChatState::default())),
            peer_state: Arc::new(RwLock::new(ChatStateNotification::Active)),
            events,
            changed: Signal::new(0),
        }
    }

    pub fn messages(&self) -> Vec<ChatMessage> {
        self.messages.read().clone()
    }

    pub fn message_count(&self) -> usize {
        self.messages.read().len()
    }

    pub fn add_message(&mut self, message: ChatMessage) {
        let mut messages = self.messages.write();
        let position = messages.partition_point(|m| m.timestamp <= message.timestamp);
        messages.insert(position, message.clone());

        // Keep only last 1000 messages in memory
        if messages.len() > 1000 {
            let drain_count = messages.len() - 1000;
            messages.drain(0..drain_count);
        }

        // Update unread count if not own message and chat not active
        if !message.is_own && *self.state.read() != ChatState::Active {
            *self.unread_count.write() += 1;
        }

        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();
    }

    /// Remember the earliest archive id seen, used to page further back.
    pub fn note_archive_id(&self, timestamp: chrono::DateTime<chrono::Utc>, id: &str) {
        let mut cursor = self.mam_cursor.write();
        if cursor
            .as_ref()
            .map(|(existing, _)| timestamp < *existing)
            .unwrap_or(true)
        {
            *cursor = Some((timestamp, id.to_string()));
        }
    }

    pub fn mam_cursor(&self) -> Option<String> {
        self.mam_cursor.read().as_ref().map(|(_, id)| id.clone())
    }

    pub fn get_message(&self, id: &str) -> Option<ChatMessage> {
        self.messages.read().iter().find(|m| m.id == id).cloned()
    }

    /// The most recent message in this chat, if any.
    pub fn last_message(&self) -> Option<ChatMessage> {
        self.messages.read().last().cloned()
    }

    pub fn update_message(&mut self, id: &str, update: MessageUpdate) -> bool {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.id == id) {
            msg.apply_update(update);
            let changed_val = *self.changed.read();
            self.changed.set(changed_val + 1);
            self.emit_changed();
            true
        } else {
            false
        }
    }

    pub fn remove_message(&mut self, id: &str) -> bool {
        let mut messages = self.messages.write();
        let len_before = messages.len();
        messages.retain(|m| m.id != id);
        let removed = messages.len() < len_before;
        if removed {
            let changed_val = *self.changed.read();
            self.changed.set(changed_val + 1);
            self.emit_changed();
        }
        removed
    }

    pub fn unread_count(&self) -> usize {
        *self.unread_count.read()
    }

    pub fn mark_read(&mut self) {
        *self.unread_count.write() = 0;
        *self.state.write() = ChatState::Active;
        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();
    }

    pub fn mark_active(&mut self) {
        *self.state.write() = ChatState::Active;
        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();
    }

    pub fn mark_inactive(&mut self) {
        *self.state.write() = ChatState::Inactive;
        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();
    }

    pub fn state(&self) -> ChatState {
        *self.state.read()
    }

    /// Chat state reported by the peer (XEP-0085).
    pub fn peer_state(&self) -> ChatStateNotification {
        *self.peer_state.read()
    }

    pub fn set_peer_state(&mut self, state: ChatStateNotification) {
        *self.peer_state.write() = state;
        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();
    }

    /// Update the delivery receipt of a message (XEP-0184 / XEP-0333).
    pub fn set_receipt(&mut self, message_id: &str, receipt: MessageReceipt) {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.id == message_id) {
            msg.receipt = receipt;
            drop(messages);
            let changed_val = *self.changed.read();
            self.changed.set(changed_val + 1);
            self.emit_changed();
        }
    }

    /// Set one sender's reactions on a message (XEP-0444). An empty list clears them.
    pub fn set_reactions_from(&mut self, message_id: &str, from_jid: &Jid, emojis: &[String]) {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.id == message_id) {
            msg.reactions.retain(|r| &r.from_jid != from_jid);
            for emoji in emojis {
                msg.reactions.push(Reaction {
                    emoji: emoji.clone(),
                    from_jid: from_jid.clone(),
                    timestamp: chrono::Utc::now(),
                });
            }
            drop(messages);
            let changed_val = *self.changed.read();
            self.changed.set(changed_val + 1);
            self.emit_changed();
        }
    }

    /// Apply a message correction (XEP-0308).
    pub fn apply_edit(&mut self, message_id: &str, new_body: &str) {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.id == message_id) {
            msg.body = Some(new_body.to_string());
            msg.is_edited = true;
            msg.edited_at = Some(chrono::Utc::now());
            drop(messages);
            let changed_val = *self.changed.read();
            self.changed.set(changed_val + 1);
            self.emit_changed();
        }
    }

    /// Retract (delete) a message (XEP-0424).
    pub fn retract(&mut self, message_id: &str) {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.id == message_id) {
            msg.is_retracted = true;
            msg.retracted_at = Some(chrono::Utc::now());
            drop(messages);
            let changed_val = *self.changed.read();
            self.changed.set(changed_val + 1);
            self.emit_changed();
        }
    }

    pub fn set_state(&mut self, state: ChatState) {
        *self.state.write() = state;
        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();
    }

    pub fn changed_signal(&self) -> Signal<u64> {
        self.changed.clone()
    }

    fn emit_changed(&self) {
        let _ = self.events.send(XmppEvent::ChatChanged {
            jid: self.bare_jid.clone(),
        });
    }
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub thread: Option<String>,
    pub bare_jid: String,
    pub from_jid: Jid,
    pub to_jid: Jid,
    pub is_own: bool,
    pub message_type: MessageType,
    pub body: Option<String>,
    pub html: Option<String>,
    pub subject: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub receipt: MessageReceipt,
    pub chat_state: Option<ChatStateNotification>,
    pub reactions: Vec<Reaction>,
    pub reply_to: Option<String>,
    pub forward_info: Option<ForwardInfo>,
    pub encryption: Option<EncryptionInfo>,
    pub media: Vec<MediaAttachment>,
    pub mentions: Vec<String>,
    pub is_edited: bool,
    pub edited_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_retracted: bool,
    pub retracted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ChatMessage {
    pub fn new(
        bare_jid: String,
        from_jid: Jid,
        to_jid: Jid,
        is_own: bool,
        body: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            thread: None,
            bare_jid,
            from_jid,
            to_jid,
            is_own,
            message_type: MessageType::Chat,
            body,
            html: None,
            subject: None,
            timestamp: chrono::Utc::now(),
            receipt: MessageReceipt::default(),
            chat_state: None,
            reactions: Vec::new(),
            reply_to: None,
            forward_info: None,
            encryption: None,
            media: Vec::new(),
            mentions: Vec::new(),
            is_edited: false,
            edited_at: None,
            is_retracted: false,
            retracted_at: None,
        }
    }

    pub fn apply_update(&mut self, update: MessageUpdate) {
        if let Some(body) = update.body {
            self.body = Some(body);
            self.is_edited = true;
            self.edited_at = Some(chrono::Utc::now());
        }
        if let Some(html) = update.html {
            self.html = Some(html);
        }
        if let Some(receipt) = update.receipt {
            self.receipt = receipt;
        }
        if let Some(reactions) = update.reactions {
            self.reactions = reactions;
        }
        if let Some(retracted) = update.retracted {
            self.is_retracted = retracted;
            if retracted {
                self.retracted_at = Some(chrono::Utc::now());
            }
        }
    }

    pub fn display_body(&self) -> String {
        if self.is_retracted {
            return "[This message was retracted]".to_string();
        }
        self.body.clone().unwrap_or_else(|| {
            if self.media.is_empty() {
                "[Empty message]".to_string()
            } else {
                format!("[{} attachment(s)]", self.media.len())
            }
        })
    }

    pub fn has_media(&self) -> bool {
        !self.media.is_empty()
    }
}

/// Message type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Chat,
    GroupChat,
    Headline,
    Error,
    Normal,
}

/// Message delivery receipt (XEP-0184)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageReceipt {
    None,
    Sent,
    Delivered,
    Displayed,
    Acknowledged,
}

impl Default for MessageReceipt {
    fn default() -> Self {
        Self::None
    }
}

/// Chat state notifications (XEP-0085)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatStateNotification {
    Active,
    Inactive,
    Composing,
    Paused,
    Gone,
}

/// Chat state for UI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ChatState {
    #[default]
    Inactive,
    Active,
    Composing,
    Paused,
}

/// Message update for editing/retracting
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageUpdate {
    pub body: Option<String>,
    pub html: Option<String>,
    pub receipt: Option<MessageReceipt>,
    pub reactions: Option<Vec<Reaction>>,
    pub retracted: Option<bool>,
}

/// Forward information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardInfo {
    pub original_jid: Jid,
    pub original_timestamp: chrono::DateTime<chrono::Utc>,
    pub original_id: String,
}

// Re-export shared types for backward compatibility
pub use crate::types::{EncryptionInfo, MediaAttachment, MediaType, Reaction};
