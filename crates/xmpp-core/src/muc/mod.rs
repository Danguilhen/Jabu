//! Multi-User Chat (MUC) management

use dioxus::prelude::{Readable, Signal, Writable};
use jid::Jid;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

use crate::{EventBus, Result, XmppEvent};
use crate::types::{EncryptionInfo, MediaAttachment, MediaType};

/// MUC manager for group chats
pub struct MucManager {
    rooms: RwLock<HashMap<String, MucRoom>>,
    events: Arc<EventBus>,
}

impl MucManager {
    pub fn new(events: Arc<EventBus>) -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
            events,
        }
    }

    pub fn get_or_create(&self, jid: &Jid) -> MucRoom {
        let bare_jid = jid.to_bare().to_string();
        let mut rooms = self.rooms.write();
        rooms
            .entry(bare_jid.clone())
            .or_insert_with(|| MucRoom::new(bare_jid, self.events.clone()))
            .clone()
    }

    pub fn get(&self, jid: &Jid) -> Option<MucRoom> {
        let bare_jid = jid.to_bare().to_string();
        self.rooms.read().get(&bare_jid).cloned()
    }

    pub fn remove(&self, jid: &Jid) -> Option<MucRoom> {
        let bare_jid = jid.to_bare().to_string();
        self.rooms.write().remove(&bare_jid)
    }

    pub fn list(&self) -> Vec<MucRoom> {
        self.rooms.read().values().cloned().collect()
    }

    pub fn list_joined(&self) -> Vec<MucRoom> {
        self.rooms
            .read()
            .values()
            .filter(|r| r.is_joined())
            .cloned()
            .collect()
    }

    pub fn join_room(&self, jid: &Jid, nick: &str, password: Option<String>) -> Result<MucRoom> {
        let mut room = self.get_or_create(jid);
        room.join(nick, password)?;
        Ok(room)
    }
}

/// MUC room
#[derive(Clone)]
pub struct MucRoom {
    pub jid: Jid,
    pub bare_jid: String,
    pub name: String,
    pub subject: Option<String>,
    pub config: MucConfig,
    occupants: Arc<RwLock<HashMap<String, MucOccupant>>>,
    messages: Arc<RwLock<Vec<MucMessage>>>,
    unread_count: Arc<RwLock<usize>>,
    state: Arc<RwLock<MucState>>,
    events: Arc<EventBus>,
    changed: Signal<u64>,
    // Self presence
    self_nick: Arc<RwLock<Option<String>>>,
    self_role: Arc<RwLock<MucRole>>,
    self_affiliation: Arc<RwLock<MucAffiliation>>,
}

impl MucRoom {
    pub fn new(bare_jid: String, events: Arc<EventBus>) -> Self {
        let jid = bare_jid.parse().unwrap();
        Self {
            jid,
            bare_jid: bare_jid.clone(),
            name: String::new(),
            subject: None,
            config: MucConfig::default(),
            occupants: Arc::new(RwLock::new(HashMap::new())),
            messages: Arc::new(RwLock::new(Vec::new())),
            unread_count: Arc::new(RwLock::new(0)),
            state: Arc::new(RwLock::new(MucState::Disconnected)),
            events,
            changed: Signal::new(0),
            self_nick: Arc::new(RwLock::new(None)),
            self_role: Arc::new(RwLock::new(MucRole::None)),
            self_affiliation: Arc::new(RwLock::new(MucAffiliation::None)),
        }
    }

    pub fn join(&mut self, nick: &str, _password: Option<String>) -> Result<()> {
        *self.self_nick.write() = Some(nick.to_string());
        *self.state.write() = MucState::Joining;

        // This would call prose-xmpp MUC join
        // For now, simulate
        *self.state.write() = MucState::Joined;

        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();

        Ok(())
    }

    pub fn leave(&mut self, _reason: Option<String>) -> Result<()> {
        *self.state.write() = MucState::Disconnected;
        self.occupants.write().clear();
        *self.self_nick.write() = None;

        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();

        Ok(())
    }

    pub fn is_joined(&self) -> bool {
        matches!(*self.state.read(), MucState::Joined | MucState::Active)
    }

    pub fn self_nick(&self) -> Option<String> {
        self.self_nick.read().clone()
    }

    pub fn self_role(&self) -> MucRole {
        *self.self_role.read()
    }

    pub fn self_affiliation(&self) -> MucAffiliation {
        *self.self_affiliation.read()
    }

    pub fn can_moderate(&self) -> bool {
        matches!(
            self.self_role(),
            MucRole::Moderator | MucRole::Owner | MucRole::Admin
        )
    }

    pub fn can_admin(&self) -> bool {
        matches!(
            self.self_affiliation(),
            MucAffiliation::Owner | MucAffiliation::Admin
        )
    }

    pub fn occupants(&self) -> Vec<MucOccupant> {
        self.occupants.read().values().cloned().collect()
    }

    pub fn occupant_count(&self) -> usize {
        self.occupants.read().len()
    }

    pub fn get_occupant(&self, nick: &str) -> Option<MucOccupant> {
        self.occupants.read().get(nick).cloned()
    }

    pub fn add_occupant(&mut self, occupant: MucOccupant) {
        self.occupants
            .write()
            .insert(occupant.nick.clone(), occupant);
        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();
    }

    pub fn remove_occupant(&mut self, nick: &str) -> Option<MucOccupant> {
        let removed = self.occupants.write().remove(nick);
        if removed.is_some() {
            let changed_val = *self.changed.read();
            self.changed.set(changed_val + 1);
            self.emit_changed();
        }
        removed
    }

    pub fn update_occupant(
        &mut self,
        nick: &str,
        role: Option<MucRole>,
        affiliation: Option<MucAffiliation>,
    ) {
        if let Some(occupant) = self.occupants.write().get_mut(nick) {
            if let Some(r) = role {
                occupant.role = r;
            }
            if let Some(a) = affiliation {
                occupant.affiliation = a;
            }
            let changed_val = *self.changed.read();
            self.changed.set(changed_val + 1);
            self.emit_changed();
        }
    }

    pub fn messages(&self) -> Vec<MucMessage> {
        self.messages.read().clone()
    }

    pub fn add_message(&mut self, message: MucMessage) {
        let mut messages = self.messages.write();
        messages.push(message.clone());

        if messages.len() > 2000 {
            let drain_count = messages.len() - 2000;
            messages.drain(0..drain_count);
        }

        if *self.state.read() != MucState::Active {
            *self.unread_count.write() += 1;
        }

        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();
    }

    pub fn unread_count(&self) -> usize {
        *self.unread_count.read()
    }

    pub fn mark_read(&mut self) {
        *self.unread_count.write() = 0;
        *self.state.write() = MucState::Active;
        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();
    }

    pub fn set_subject(&mut self, subject: String, _by: Option<Jid>) {
        self.subject = Some(subject);
        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();
    }

    pub fn changed_signal(&self) -> Signal<u64> {
        self.changed.clone()
    }

    fn emit_changed(&self) {
        let _ = self.events.send(XmppEvent::MucChanged {
            jid: self.bare_jid.clone(),
        });
    }
}

/// MUC room configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MucConfig {
    pub is_persistent: bool,
    pub is_members_only: bool,
    pub is_moderated: bool,
    pub is_password_protected: bool,
    pub password: Option<String>,
    pub max_occupants: Option<u32>,
    pub allow_invites: bool,
    pub allow_visitor_status: bool,
    pub allow_visitor_nickchange: bool,
    pub allow_private_messages: bool,
    pub allow_query_users: bool,
    pub log_enabled: bool,
    pub language: Option<String>,
}

impl Default for MucConfig {
    fn default() -> Self {
        Self {
            is_persistent: true,
            is_members_only: false,
            is_moderated: false,
            is_password_protected: false,
            password: None,
            max_occupants: None,
            allow_invites: true,
            allow_visitor_status: true,
            allow_visitor_nickchange: true,
            allow_private_messages: true,
            allow_query_users: true,
            log_enabled: true,
            language: Some("en".to_string()),
        }
    }
}

/// MUC room state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MucState {
    Disconnected,
    Joining,
    Joined,
    Active,
    Left,
    Error,
}

/// MUC occupant (participant in the room)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MucOccupant {
    pub nick: String,
    pub jid: Option<Jid>, // Real JID if visible (non-anonymous room)
    pub role: MucRole,
    pub affiliation: MucAffiliation,
    pub presence: MucPresence,
    pub joined_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl MucOccupant {
    pub fn new(nick: String) -> Self {
        Self {
            nick,
            jid: None,
            role: MucRole::Participant,
            affiliation: MucAffiliation::None,
            presence: MucPresence::default(),
            joined_at: Some(chrono::Utc::now()),
        }
    }

    pub fn is_self(&self, self_nick: &str) -> bool {
        self.nick == self_nick
    }
}

/// MUC presence
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MucPresence {
    pub show: PresenceShow,
    pub status: Option<String>,
    pub idle_since: Option<chrono::DateTime<chrono::Utc>>,
}

/// MUC role (temporary, based on presence)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MucRole {
    None,
    Visitor,
    Participant,
    Moderator,
    Owner,
    Admin,
}

impl MucRole {
    pub fn hierarchy(&self) -> u8 {
        match self {
            Self::None => 0,
            Self::Visitor => 1,
            Self::Participant => 2,
            Self::Moderator => 3,
            Self::Admin => 4,
            Self::Owner => 5,
        }
    }

    pub fn can_kick(&self, target: &Self) -> bool {
        self.hierarchy() > target.hierarchy()
    }
}

/// MUC affiliation (persistent, based on room config)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MucAffiliation {
    None,
    Member,
    Admin,
    Owner,
    Outcast, // Banned
}

impl MucAffiliation {
    pub fn can_assign(&self, target: &Self) -> bool {
        matches!(
            (self, target),
            (Self::Owner, _) | (Self::Admin, Self::Member | Self::None | Self::Outcast)
        )
    }
}

/// MUC message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MucMessage {
    pub id: String,
    pub room_jid: String,
    pub from_nick: String,
    pub from_jid: Option<Jid>,
    pub is_own: bool,
    pub body: Option<String>,
    pub html: Option<String>,
    pub subject: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub reactions: Vec<Reaction>,
    pub reply_to: Option<String>,
    pub encryption: Option<EncryptionInfo>,
    pub media: Vec<MediaAttachment>,
    pub mentions: Vec<String>,
    pub is_edited: bool,
    pub edited_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_retracted: bool,
    pub retracted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl MucMessage {
    pub fn new(
        room_jid: String,
        from_nick: String,
        from_jid: Option<Jid>,
        is_own: bool,
        body: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            room_jid,
            from_nick,
            from_jid,
            is_own,
            body,
            html: None,
            subject: None,
            timestamp: chrono::Utc::now(),
            reactions: Vec::new(),
            reply_to: None,
            encryption: None,
            media: Vec::new(),
            mentions: Vec::new(),
            is_edited: false,
            edited_at: None,
            is_retracted: false,
            retracted_at: None,
        }
    }
}

/// Reaction to a message (MUC-specific, includes from_nick)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    pub from_nick: String,
    pub from_jid: Option<Jid>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Presence show values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PresenceShow {
    #[default]
    Online,
    Away,
    Dnd,
    Xa,
    Unavailable,
}