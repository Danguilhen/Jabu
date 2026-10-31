//! 1:1 Chat management

use jid::Jid;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::change::ChangeCounter;
use crate::{EventBus, XmppEvent};

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

    /// Drop every conversation, e.g. when switching accounts.
    pub fn clear(&self) {
        self.chats.write().clear();
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
    preferences: Arc<RwLock<ChatPreferences>>,
    events: Arc<EventBus>,
    changed: Arc<ChangeCounter>,
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
            preferences: Arc::new(RwLock::new(ChatPreferences::default())),
            events,
            changed: Arc::new(ChangeCounter::default()),
        }
    }

    pub fn messages(&self) -> Vec<ChatMessage> {
        self.messages.read().clone()
    }

    pub fn message_count(&self) -> usize {
        self.messages.read().len()
    }

    /// Insert a message loaded from storage without emitting events or changing unread state.
    pub fn insert_loaded(&mut self, message: ChatMessage) {
        let mut messages = self.messages.write();
        if messages.iter().any(|m| m.id == message.id) {
            return;
        }
        let position = messages.partition_point(|m| m.timestamp <= message.timestamp);
        messages.insert(position, message);
        if messages.len() > 1000 {
            let drain_count = messages.len() - 1000;
            messages.drain(0..drain_count);
        }
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

        let event = if message.is_own {
            XmppEvent::MessageSent {
                jid: self.bare_jid.clone(),
                message: message.clone(),
            }
        } else {
            XmppEvent::MessageReceived {
                jid: self.bare_jid.clone(),
                message: message.clone(),
            }
        };
        let _ = self.events.send(event);

        self.changed.bump();
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

    /// Find a message by its local id, origin-id or server stanza-id.
    #[must_use]
    pub fn find_message(&self, id: &str) -> Option<ChatMessage> {
        self.messages
            .read()
            .iter()
            .find(|m| m.matches_id(id))
            .cloned()
    }

    /// Whether any of the given ids is already in this conversation.
    #[must_use]
    pub fn contains_any_id<'a>(&self, ids: impl IntoIterator<Item = &'a str>) -> bool {
        let ids: Vec<&str> = ids.into_iter().collect();
        self.messages
            .read()
            .iter()
            .any(|m| ids.iter().any(|id| m.matches_id(id)))
    }

    /// Store the archive id to page before, replacing the cursor outright.
    pub fn set_mam_cursor(&self, id: &str) {
        if id.is_empty() {
            return;
        }
        let mut cursor = self.mam_cursor.write();
        *cursor = Some((chrono::Utc::now(), id.to_string()));
    }

    /// The most recent message in this chat, if any.
    pub fn last_message(&self) -> Option<ChatMessage> {
        self.messages.read().last().cloned()
    }

    pub fn update_message(&mut self, id: &str, update: MessageUpdate) -> bool {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.id == id) {
            msg.apply_update(update);
            self.changed.bump();
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
            self.changed.bump();
            self.emit_changed();
        }
        removed
    }

    pub fn unread_count(&self) -> usize {
        *self.unread_count.read()
    }

    pub fn preferences(&self) -> ChatPreferences {
        *self.preferences.read()
    }

    pub fn set_preferences(&mut self, preferences: ChatPreferences) {
        *self.preferences.write() = preferences;
        self.changed.bump();
        self.emit_changed();
    }

    /// Apply preferences loaded from storage without emitting a change event.
    pub fn restore_preferences(&mut self, preferences: ChatPreferences) {
        *self.preferences.write() = preferences;
    }

    pub fn set_pinned(&mut self, pinned: bool) {
        self.preferences.write().is_pinned = pinned;
        self.changed.bump();
        self.emit_changed();
    }

    pub fn set_archived(&mut self, archived: bool) {
        self.preferences.write().is_archived = archived;
        self.changed.bump();
        self.emit_changed();
    }

    pub fn set_notifications_enabled(&mut self, enabled: bool) {
        self.preferences.write().notifications_enabled = enabled;
        self.changed.bump();
        self.emit_changed();
    }

    pub fn mark_read(&mut self) {
        *self.unread_count.write() = 0;
        *self.state.write() = ChatState::Active;
        self.changed.bump();
        self.emit_changed();
    }

    /// Drop the in-memory history and paging cursor.
    pub fn clear_messages(&mut self) {
        self.messages.write().clear();
        *self.mam_cursor.write() = None;
        *self.unread_count.write() = 0;
        self.changed.bump();
        self.emit_changed();
    }

    pub fn mark_active(&mut self) {
        *self.state.write() = ChatState::Active;
        self.changed.bump();
        self.emit_changed();
    }

    pub fn mark_inactive(&mut self) {
        *self.state.write() = ChatState::Inactive;
        self.changed.bump();
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
        self.changed.bump();
        self.emit_changed();
    }

    /// Update the delivery receipt of a message (XEP-0184 / XEP-0333).
    pub fn set_receipt(&mut self, message_id: &str, receipt: MessageReceipt) {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.matches_id(message_id)) {
            let id = msg.id.clone();
            msg.receipt = receipt;
            drop(messages);
            let _ = self.events.send(XmppEvent::MessageReceipt {
                jid: self.bare_jid.clone(),
                message_id: id,
                receipt,
            });
            self.changed.bump();
            self.emit_changed();
        }
    }

    /// Set one sender's reactions on a message (XEP-0444). An empty list clears them.
    pub fn set_reactions_from(&mut self, message_id: &str, from_jid: &Jid, emojis: &[String]) {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.matches_id(message_id)) {
            let id = msg.id.clone();
            let previous: Vec<String> = msg
                .reactions
                .iter()
                .filter(|r| &r.from_jid == from_jid)
                .map(|r| r.emoji.clone())
                .collect();
            msg.reactions.retain(|r| &r.from_jid != from_jid);
            for emoji in emojis {
                msg.reactions.push(Reaction {
                    emoji: emoji.clone(),
                    from_jid: from_jid.clone(),
                    timestamp: chrono::Utc::now(),
                });
            }
            drop(messages);
            for emoji in &previous {
                if !emojis.contains(emoji) {
                    let _ = self.events.send(XmppEvent::ReactionRemoved {
                        jid: self.bare_jid.clone(),
                        message_id: id.clone(),
                        reaction: Reaction {
                            emoji: emoji.clone(),
                            from_jid: from_jid.clone(),
                            timestamp: chrono::Utc::now(),
                        },
                    });
                }
            }
            for emoji in emojis {
                if !previous.contains(emoji) {
                    let _ = self.events.send(XmppEvent::ReactionAdded {
                        jid: self.bare_jid.clone(),
                        message_id: id.clone(),
                        reaction: Reaction {
                            emoji: emoji.clone(),
                            from_jid: from_jid.clone(),
                            timestamp: chrono::Utc::now(),
                        },
                    });
                }
            }
            self.changed.bump();
            self.emit_changed();
        }
    }

    /// Apply a message correction (XEP-0308).
    pub fn apply_edit(&mut self, message_id: &str, new_body: &str) {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.matches_id(message_id)) {
            let id = msg.id.clone();
            msg.body = Some(new_body.to_string());
            msg.is_edited = true;
            msg.edited_at = Some(chrono::Utc::now());
            drop(messages);
            let _ = self.events.send(XmppEvent::MessageUpdated {
                jid: self.bare_jid.clone(),
                message_id: id,
            });
            self.changed.bump();
            self.emit_changed();
        }
    }

    /// Retract (delete) a message (XEP-0424).
    pub fn retract(&mut self, message_id: &str) {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.matches_id(message_id)) {
            let id = msg.id.clone();
            msg.is_retracted = true;
            msg.retracted_at = Some(chrono::Utc::now());
            drop(messages);
            let _ = self.events.send(XmppEvent::MessageDeleted {
                jid: self.bare_jid.clone(),
                message_id: id,
            });
            self.changed.bump();
            self.emit_changed();
        }
    }

    pub fn set_state(&mut self, state: ChatState) {
        *self.state.write() = state;
        self.changed.bump();
        self.emit_changed();
    }

    fn emit_changed(&self) {
        let _ = self.events.send(XmppEvent::ChatChanged {
            jid: self.bare_jid.clone(),
        });
    }
}

/// Per-conversation preferences kept across restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatPreferences {
    pub is_pinned: bool,
    pub is_archived: bool,
    pub notifications_enabled: bool,
}

impl Default for ChatPreferences {
    fn default() -> Self {
        Self {
            is_pinned: false,
            is_archived: false,
            notifications_enabled: true,
        }
    }
}

/// XEP-0334 message-processing hints carried by a stanza.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageHints {
    pub store: bool,
    pub no_store: bool,
    pub no_copy: bool,
    pub markable: bool,
}

/// A completed or missed 1:1 call shown inline in the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallEventInfo {
    pub direction: String,
    pub kind: String,
    pub state: String,
    pub duration_secs: i64,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    #[serde(default)]
    pub origin_id: Option<String>,
    #[serde(default)]
    pub stanza_id: Option<String>,
    #[serde(default)]
    pub hints: MessageHints,
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
    #[serde(default)]
    pub call: Option<CallEventInfo>,
    #[serde(default)]
    pub story_ref: Option<crate::microblog::StoryRef>,
    #[serde(default)]
    pub sticker_cid: Option<String>,
    #[serde(default)]
    pub sticker_alt: Option<String>,
    pub mentions: Vec<String>,
    #[serde(default)]
    pub attention: bool,
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
            origin_id: None,
            stanza_id: None,
            hints: MessageHints::default(),
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
            call: None,
            story_ref: None,
            sticker_cid: None,
            sticker_alt: None,
            mentions: Vec::new(),
            attention: false,
            is_edited: false,
            edited_at: None,
            is_retracted: false,
            retracted_at: None,
        }
    }

    /// Whether `id` refers to this message in any of its identities.
    pub fn matches_id(&self, id: &str) -> bool {
        self.id == id
            || self.origin_id.as_deref() == Some(id)
            || self.stanza_id.as_deref() == Some(id)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn chat() -> Chat {
        Chat::new("bob@example.org".into(), Arc::new(EventBus::new()))
    }

    fn message(chat: &Chat, id: &str, origin: &str, stanza: &str) -> ChatMessage {
        let mut message = ChatMessage::new(
            chat.bare_jid.clone(),
            chat.jid.clone(),
            chat.jid.clone(),
            false,
            Some("hello".into()),
        );
        message.id = id.into();
        message.origin_id = Some(origin.into());
        message.stanza_id = Some(stanza.into());
        message
    }

    #[test]
    fn preferences_start_enabled_and_toggle_independently() {
        let mut chat = chat();
        assert_eq!(chat.preferences(), ChatPreferences::default());
        chat.set_pinned(true);
        chat.set_archived(true);
        chat.set_notifications_enabled(false);
        let prefs = chat.preferences();
        assert!(prefs.is_pinned);
        assert!(prefs.is_archived);
        assert!(!prefs.notifications_enabled);
    }

    #[test]
    fn matches_any_message_identity() {
        let mut chat = chat();
        chat.add_message(message(&chat.clone(), "local-1", "origin-1", "stanza-1"));

        assert!(chat.contains_any_id(["origin-1"]));
        assert!(chat.contains_any_id(["stanza-1"]));
        assert!(!chat.contains_any_id(["other"]));
        assert_eq!(chat.find_message("stanza-1").unwrap().id, "local-1");
    }

    #[test]
    fn receipts_and_edits_accept_origin_ids() {
        let mut chat = chat();
        chat.add_message(message(&chat.clone(), "local-1", "origin-1", "stanza-1"));

        chat.set_receipt("origin-1", MessageReceipt::Displayed);
        assert_eq!(
            chat.find_message("local-1").unwrap().receipt,
            MessageReceipt::Displayed
        );

        chat.apply_edit("stanza-1", "fixed");
        let edited = chat.find_message("local-1").unwrap();
        assert_eq!(edited.body.as_deref(), Some("fixed"));
        assert!(edited.is_edited);
    }

    #[test]
    fn own_messages_are_not_unread() {
        let mut chat = chat();
        let mut own = message(&chat.clone(), "own-1", "own-1", "own-1");
        own.is_own = true;
        chat.add_message(own);
        assert_eq!(chat.unread_count(), 0);
    }
}
