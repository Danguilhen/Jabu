//! Multi-User Chat (MUC) management

use jid::Jid;
use minidom::Element;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::change::ChangeCounter;
use crate::chat::MessageHints;
use crate::types::{EncryptionInfo, MediaAttachment};
use crate::{EventBus, Result, XmppEvent};

/// Build the XEP-0410 self-ping for our own occupant JID in a room.
///
/// The target is `room@conference/nick`; an IQ to that address is routed by
/// the room to the occupant, and a failure means the room session is gone.
#[must_use]
pub fn self_ping_iq(target: &Jid) -> Element {
    Element::builder("iq", "jabber:client")
        .attr("type", "get")
        .attr("to", target.to_string())
        .attr("id", format!("muc-self-ping-{}", Uuid::new_v4()))
        .append(Element::builder("ping", "urn:xmpp:ping"))
        .build()
}

/// Build a XEP-0402 bookmark item, optionally pinned with XEP-0469.
#[must_use]
pub fn bookmark_item(room: &Jid, name: &str, nick: &str, autojoin: bool, pinned: bool) -> Element {
    let mut conference = Element::builder("conference", crate::spaces::NS_BOOKMARKS)
        .attr("name", name.to_string())
        .attr("autojoin", if autojoin { "true" } else { "false" });
    if !nick.trim().is_empty() {
        conference = conference.append(
            Element::builder("nick", crate::spaces::NS_BOOKMARKS).append(nick.trim().to_string()),
        );
    }
    if pinned {
        conference = conference.append(
            Element::builder("extensions", crate::spaces::NS_BOOKMARKS)
                .append(Element::builder("pinned", crate::spaces::NS_PINNED)),
        );
    }
    Element::builder("item", "http://jabber.org/protocol/pubsub")
        .attr("id", room.to_bare().to_string())
        .append(conference)
        .build()
}

/// Build a XEP-0425 moderation request for another occupant's message.
#[must_use]
pub fn moderation_message(room: &Jid, message_id: &str, reason: Option<&str>) -> Element {
    let mut moderated = Element::builder("moderated", "urn:xmpp:message-moderate:1")
        .append(Element::builder("retract", "urn:xmpp:message-moderate:1"));
    if let Some(reason) = reason.filter(|reason| !reason.trim().is_empty()) {
        moderated = moderated.append(
            Element::builder("reason", "urn:xmpp:message-moderate:1").append(reason.trim()),
        );
    }
    Element::builder("message", "jabber:client")
        .attr("type", "groupchat")
        .attr("to", room.to_bare().to_string())
        .attr("id", format!("moderate-{}", Uuid::new_v4()))
        .append(
            Element::builder("apply-to", "urn:xmpp:fasten:0")
                .attr("id", message_id.to_string())
                .append(moderated),
        )
        .build()
}

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

    /// Drop every room, e.g. when switching accounts.
    pub fn clear(&self) {
        self.rooms.write().clear();
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

    /// Look up a room by its bare JID string (as stored in the map).
    pub fn get_by_bare(&self, bare: &str) -> Option<MucRoom> {
        self.rooms.read().get(bare).cloned()
    }

    /// Update the stored room's subject. Mutating a cloned room would not persist.
    pub fn set_subject(&self, bare: &str, subject: String, by: Option<Jid>) {
        if let Some(room) = self.rooms.write().get_mut(bare) {
            room.set_subject(subject, by);
        }
    }

    /// Restore saved metadata into the stored room.
    pub fn restore_meta(&self, bare: &str, name: String, nick: String) {
        if let Some(room) = self.rooms.write().get_mut(bare) {
            room.restore_meta(name, nick);
        }
    }

    /// Apply the bookmark flags of a stored room without re-emitting events.
    pub fn restore_bookmark(&self, bare: &str, autojoin: bool, pinned: bool, bookmarked: bool) {
        if let Some(room) = self.rooms.write().get_mut(bare) {
            *room.autojoin.write() = autojoin;
            *room.pinned.write() = pinned;
            *room.bookmarked.write() = bookmarked;
        }
    }

    /// Update the stored room after a bookmark publication.
    pub fn set_bookmark(
        &self,
        bare: &str,
        name: String,
        nick: String,
        autojoin: bool,
        pinned: bool,
    ) {
        if let Some(room) = self.rooms.write().get_mut(bare) {
            room.set_bookmark(name, nick, autojoin, pinned, true);
        }
    }

    pub fn clear_bookmark(&self, bare: &str) {
        if let Some(room) = self.rooms.write().get_mut(bare) {
            room.clear_bookmark();
        }
    }

    /// The stream ended: every room that was live needs rejoining later.
    pub fn mark_all_disconnected(&self) {
        for room in self.rooms.read().values() {
            room.mark_disconnected();
        }
    }

    /// Rooms that were live when the stream ended, with the nick to rejoin as.
    pub fn take_rejoin_rooms(&self) -> Vec<(Jid, String)> {
        self.rooms
            .read()
            .values()
            .filter_map(|room| room.take_rejoin_nick().map(|nick| (room.jid.clone(), nick)))
            .collect()
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
    mam_cursor: Arc<RwLock<Option<(chrono::DateTime<chrono::Utc>, String)>>>,
    unread_count: Arc<RwLock<usize>>,
    state: Arc<RwLock<MucState>>,
    events: Arc<EventBus>,
    changed: Arc<ChangeCounter>,
    // Self presence
    self_nick: Arc<RwLock<Option<String>>>,
    self_role: Arc<RwLock<MucRole>>,
    self_affiliation: Arc<RwLock<MucAffiliation>>,
    /// Whether the room must be joined again after the stream comes back.
    rejoin_on_connect: Arc<RwLock<bool>>,
    /// Whether the room is stored in our XEP-0402 bookmark list.
    bookmarked: Arc<RwLock<bool>>,
    /// Whether the bookmark asks the client to join automatically.
    autojoin: Arc<RwLock<bool>>,
    /// Whether the bookmark is pinned (XEP-0469).
    pinned: Arc<RwLock<bool>>,
    /// Cached MUC member/admin/owner affiliations for OMEMO recipient discovery.
    affiliations: Arc<RwLock<HashMap<String, MucAffiliation>>>,
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
            mam_cursor: Arc::new(RwLock::new(None)),
            unread_count: Arc::new(RwLock::new(0)),
            state: Arc::new(RwLock::new(MucState::Disconnected)),
            events,
            changed: Arc::new(ChangeCounter::default()),
            self_nick: Arc::new(RwLock::new(None)),
            self_role: Arc::new(RwLock::new(MucRole::None)),
            self_affiliation: Arc::new(RwLock::new(MucAffiliation::None)),
            rejoin_on_connect: Arc::new(RwLock::new(false)),
            bookmarked: Arc::new(RwLock::new(false)),
            autojoin: Arc::new(RwLock::new(false)),
            pinned: Arc::new(RwLock::new(false)),
            affiliations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn join(&mut self, nick: &str, _password: Option<String>) -> Result<()> {
        *self.self_nick.write() = Some(nick.to_string());
        *self.state.write() = MucState::Joining;
        self.changed.bump();
        self.emit_changed();
        Ok(())
    }

    /// Mark the room joined once the server echoes our own presence.
    pub fn mark_joined(&self) {
        *self.state.write() = MucState::Joined;
        self.changed.bump();
        self.emit_changed();
    }

    pub fn set_self_role(&self, role: MucRole) {
        *self.self_role.write() = role;
        self.changed.bump();
        self.emit_changed();
    }

    pub fn set_self_affiliation(&self, affiliation: MucAffiliation) {
        *self.self_affiliation.write() = affiliation;
        self.changed.bump();
        self.emit_changed();
    }

    pub fn leave(&mut self, _reason: Option<String>) -> Result<()> {
        *self.state.write() = MucState::Disconnected;
        self.occupants.write().clear();
        *self.self_nick.write() = None;

        self.changed.bump();
        self.emit_changed();

        Ok(())
    }

    pub fn is_joined(&self) -> bool {
        matches!(*self.state.read(), MucState::Joined | MucState::Active)
    }

    /// Whether a join has been sent and the room is waiting for our presence echo.
    pub fn is_joining(&self) -> bool {
        matches!(*self.state.read(), MucState::Joining)
    }

    /// Whether the room is known but no join is in flight or in effect.
    ///
    /// A room restored from storage (or left behind by a previous run) is
    /// `Disconnected`, so opening it has to send presence again; otherwise the
    /// client only shows its own local echoes and never receives room traffic.
    pub fn needs_join(&self) -> bool {
        matches!(
            *self.state.read(),
            MucState::Disconnected | MucState::Left | MucState::Error
        )
    }

    /// The stream ended: our server-side session is gone, so the room must be
    /// joined again when the client comes back. The nick is kept for that.
    ///
    /// Does nothing for a room that was not joined, so a reconnect does not
    /// resurrect rooms the user never opened.
    pub fn mark_disconnected(&self) {
        if !self.is_joined() && !self.is_joining() {
            return;
        }
        *self.rejoin_on_connect.write() = true;
        *self.state.write() = MucState::Disconnected;
        self.occupants.write().clear();
        self.changed.bump();
        self.emit_changed();
    }

    /// Take the nick to rejoin with, if the stream ended while in this room.
    pub fn take_rejoin_nick(&self) -> Option<String> {
        let mut pending = self.rejoin_on_connect.write();
        if !*pending {
            return None;
        }
        *pending = false;
        self.self_nick.read().clone()
    }

    pub fn self_nick(&self) -> Option<String> {
        self.self_nick.read().clone()
    }

    /// Our own occupant JID while the room is joined, for XEP-0410.
    #[must_use]
    pub fn self_ping_target(&self) -> Option<Jid> {
        if !self.is_joined() {
            return None;
        }
        let nick = self.self_nick()?;
        format!("{}/{}", self.jid.to_bare(), nick).parse().ok()
    }

    pub fn self_role(&self) -> MucRole {
        *self.self_role.read()
    }

    pub fn self_affiliation(&self) -> MucAffiliation {
        *self.self_affiliation.read()
    }

    /// Replace the cached list for one affiliation from a muc#admin result.
    pub fn set_affiliation_list(&self, affiliation: MucAffiliation, jids: Vec<String>) {
        let mut affiliations = self.affiliations.write();
        affiliations.retain(|_, value| *value != affiliation);
        for jid in jids {
            if let Ok(jid) = jid.parse::<Jid>() {
                affiliations.insert(jid.to_bare().to_string(), affiliation);
            }
        }
        drop(affiliations);
        self.changed.bump();
        self.emit_changed();
    }

    /// Bare JIDs from the cached member/admin/owner lists.
    #[must_use]
    pub fn affiliation_jids(&self) -> Vec<Jid> {
        self.affiliations
            .read()
            .keys()
            .filter_map(|jid| jid.parse().ok())
            .collect()
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
        self.changed.bump();
        self.emit_changed();
    }

    pub fn remove_occupant(&mut self, nick: &str) -> Option<MucOccupant> {
        let removed = self.occupants.write().remove(nick);
        if removed.is_some() {
            self.changed.bump();
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
            self.changed.bump();
            self.emit_changed();
        }
    }

    pub fn messages(&self) -> Vec<MucMessage> {
        self.messages.read().clone()
    }

    /// Find a message by its local id, origin-id or server stanza-id.
    #[must_use]
    pub fn find_message(&self, id: &str) -> Option<MucMessage> {
        self.messages
            .read()
            .iter()
            .find(|m| m.matches_id(id))
            .cloned()
    }

    /// Whether any of the given ids is already in this room.
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

    /// The most recent message in this room, if any.
    pub fn last_message(&self) -> Option<MucMessage> {
        self.messages.read().last().cloned()
    }

    /// Insert a message loaded from storage without emitting events or changing unread state.
    pub fn insert_loaded(&mut self, message: MucMessage) {
        let mut messages = self.messages.write();
        if messages.iter().any(|m| m.id == message.id) {
            return;
        }
        let position = messages.partition_point(|m| m.timestamp <= message.timestamp);
        messages.insert(position, message);
        if messages.len() > 2000 {
            let drain_count = messages.len() - 2000;
            messages.drain(0..drain_count);
        }
    }

    /// Restore saved room metadata without pretending the room is joined.
    pub fn restore_meta(&mut self, name: String, nick: String) {
        self.name = name;
        *self.self_nick.write() = Some(nick);
        *self.state.write() = MucState::Disconnected;
    }

    /// Remember that this room is bookmarked, and with which flags.
    pub fn set_bookmark(
        &mut self,
        name: String,
        nick: String,
        autojoin: bool,
        pinned: bool,
        bookmarked: bool,
    ) {
        if !name.is_empty() {
            self.name = name;
        }
        if !nick.is_empty() {
            *self.self_nick.write() = Some(nick);
        }
        *self.autojoin.write() = autojoin;
        *self.pinned.write() = pinned;
        *self.bookmarked.write() = bookmarked;
        self.changed.bump();
        self.emit_changed();
    }

    /// Forget the bookmark while keeping the room in the list if it is joined.
    pub fn clear_bookmark(&mut self) {
        *self.bookmarked.write() = false;
        *self.pinned.write() = false;
        *self.autojoin.write() = false;
        self.changed.bump();
        self.emit_changed();
    }

    #[must_use]
    pub fn is_bookmarked(&self) -> bool {
        *self.bookmarked.read()
    }

    #[must_use]
    pub fn is_autojoin(&self) -> bool {
        *self.autojoin.read()
    }

    #[must_use]
    pub fn is_pinned(&self) -> bool {
        *self.pinned.read()
    }

    /// Detect our own message echoed back when it carried no usable id.
    #[must_use]
    pub fn is_echo(&self, nick: &str, body: &str) -> bool {
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(60);
        self.messages.read().iter().rev().take(5).any(|m| {
            m.is_own
                && m.from_nick == nick
                && m.body.as_deref() == Some(body)
                && m.timestamp >= cutoff
        })
    }

    pub fn add_message(&mut self, message: MucMessage) {
        let mut messages = self.messages.write();
        let position = messages.partition_point(|m| m.timestamp <= message.timestamp);
        messages.insert(position, message.clone());

        if messages.len() > 2000 {
            let drain_count = messages.len() - 2000;
            messages.drain(0..drain_count);
        }

        if !message.is_own && *self.state.read() != MucState::Active {
            *self.unread_count.write() += 1;
        }

        let _ = self.events.send(XmppEvent::MucMessageReceived {
            jid: self.bare_jid.clone(),
            message: message.clone(),
        });

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

    pub fn unread_count(&self) -> usize {
        *self.unread_count.read()
    }

    /// Apply a message correction (XEP-0308) to a room message.
    pub fn apply_edit(&mut self, message_id: &str, new_body: &str) {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.matches_id(message_id)) {
            msg.body = Some(new_body.to_string());
            msg.is_edited = true;
            msg.edited_at = Some(chrono::Utc::now());
            drop(messages);
            self.changed.bump();
            self.emit_changed();
        }
    }

    /// Mark a room message retracted (XEP-0424).
    pub fn retract(&mut self, message_id: &str) {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.matches_id(message_id)) {
            msg.is_retracted = true;
            msg.retracted_at = Some(chrono::Utc::now());
            drop(messages);
            self.changed.bump();
            self.emit_changed();
        }
    }

    /// Set the reactions of one occupant on a room message (XEP-0444).
    pub fn set_reactions_from(&mut self, message_id: &str, nick: &str, emojis: &[String]) {
        let mut messages = self.messages.write();
        if let Some(msg) = messages.iter_mut().find(|m| m.matches_id(message_id)) {
            msg.reactions.retain(|r| r.from_nick != nick);
            for emoji in emojis {
                msg.reactions.push(Reaction {
                    emoji: emoji.clone(),
                    from_nick: nick.to_string(),
                    from_jid: None,
                    timestamp: chrono::Utc::now(),
                });
            }
            drop(messages);
            self.changed.bump();
            self.emit_changed();
        }
    }

    pub fn mark_read(&mut self) {
        *self.unread_count.write() = 0;
        *self.state.write() = MucState::Active;
        self.changed.bump();
        self.emit_changed();
    }

    /// Drop the in-memory room history and paging cursor.
    pub fn clear_messages(&mut self) {
        self.messages.write().clear();
        *self.mam_cursor.write() = None;
        *self.unread_count.write() = 0;
        self.changed.bump();
        self.emit_changed();
    }

    pub fn set_subject(&mut self, subject: String, _by: Option<Jid>) {
        self.subject = Some(subject);
        self.changed.bump();
        self.emit_changed();
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    #[serde(default)]
    pub origin_id: Option<String>,
    #[serde(default)]
    pub stanza_id: Option<String>,
    #[serde(default)]
    pub hints: MessageHints,
    #[serde(default)]
    pub thread: Option<String>,
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
    #[serde(default)]
    pub forward_info: Option<crate::chat::ForwardInfo>,
    pub encryption: Option<EncryptionInfo>,
    pub media: Vec<MediaAttachment>,
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

impl MucMessage {
    /// Whether `id` refers to this message in any of its identities.
    pub fn matches_id(&self, id: &str) -> bool {
        self.id == id
            || self.origin_id.as_deref() == Some(id)
            || self.stanza_id.as_deref() == Some(id)
    }

    pub fn new(
        room_jid: String,
        from_nick: String,
        from_jid: Option<Jid>,
        is_own: bool,
        body: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            origin_id: None,
            stanza_id: None,
            hints: MessageHints::default(),
            thread: None,
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
            forward_info: None,
            encryption: None,
            media: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> MucRoom {
        MucRoom::new(
            "test@conference.example.org".to_string(),
            Arc::new(EventBus::new()),
        )
    }

    #[test]
    fn a_restored_room_needs_a_join_again() {
        let mut room = room();
        // Storage restores metadata and the nick but not the session, so this is
        // the state the UI sees for a room it knew from a previous run.
        room.restore_meta("Test".to_string(), "alice".to_string());
        assert!(room.needs_join());

        room.join("alice", None).expect("join");
        assert!(room.is_joining());
        assert!(!room.needs_join());

        room.mark_joined();
        assert!(room.is_joined());
        assert!(!room.needs_join());
    }

    #[test]
    fn self_ping_uses_our_joined_occupant_jid() {
        let mut room = room();
        assert!(room.self_ping_target().is_none());

        room.join("alice", None).expect("join");
        assert!(
            room.self_ping_target().is_none(),
            "a joining room has no session to ping yet"
        );

        room.mark_joined();
        let target = room.self_ping_target().expect("target");
        assert_eq!(target.to_string(), "test@conference.example.org/alice");
    }

    #[test]
    fn self_ping_iq_targets_the_occupant() {
        let target: Jid = "test@conference.example.org/alice".parse().expect("jid");
        let iq = self_ping_iq(&target);
        assert_eq!(iq.name(), "iq");
        assert_eq!(iq.ns(), "jabber:client");
        assert_eq!(iq.attr("type"), Some("get"));
        assert_eq!(iq.attr("to"), Some("test@conference.example.org/alice"));
        assert!(iq.get_child("ping", "urn:xmpp:ping").is_some());
    }

    #[test]
    fn moderation_request_targets_the_original_message() {
        let room: Jid = "test@conference.example.org".parse().expect("jid");
        let message = moderation_message(&room, "m1", Some("spam"));
        assert_eq!(message.attr("type"), Some("groupchat"));
        assert_eq!(message.attr("to"), Some("test@conference.example.org"));
        let apply_to = message
            .get_child("apply-to", "urn:xmpp:fasten:0")
            .expect("apply-to");
        assert_eq!(apply_to.attr("id"), Some("m1"));
        let moderated = apply_to
            .get_child("moderated", "urn:xmpp:message-moderate:1")
            .expect("moderated");
        assert!(moderated
            .get_child("retract", "urn:xmpp:message-moderate:1")
            .is_some());
        assert_eq!(
            moderated
                .get_child("reason", "urn:xmpp:message-moderate:1")
                .map(|reason| reason.text()),
            Some("spam".to_string())
        );
    }

    #[test]
    fn bookmark_item_carries_the_pin_when_requested() {
        let room: Jid = "test@conference.example.org".parse().expect("jid");
        let item = bookmark_item(&room, "Test", "alice", true, true);
        assert_eq!(item.attr("id"), Some("test@conference.example.org"));
        let conference = item
            .get_child("conference", crate::spaces::NS_BOOKMARKS)
            .expect("conference");
        assert_eq!(conference.attr("name"), Some("Test"));
        assert_eq!(conference.attr("autojoin"), Some("true"));
        assert_eq!(
            conference
                .get_child("nick", crate::spaces::NS_BOOKMARKS)
                .map(|nick| nick.text()),
            Some("alice".to_string())
        );
        let extensions = conference
            .get_child("extensions", crate::spaces::NS_BOOKMARKS)
            .expect("extensions");
        assert!(extensions
            .get_child("pinned", crate::spaces::NS_PINNED)
            .is_some());

        let unpinned = bookmark_item(&room, "Test", "alice", false, false);
        let conference = unpinned
            .get_child("conference", crate::spaces::NS_BOOKMARKS)
            .expect("conference");
        assert_eq!(conference.attr("autojoin"), Some("false"));
        assert!(
            conference
                .get_child("extensions", crate::spaces::NS_BOOKMARKS)
                .is_none(),
            "an unpinned bookmark has no extensions element"
        );
    }

    #[test]
    fn bookmark_flags_are_tracked_on_the_room() {
        let mut room = room();
        assert!(!room.is_bookmarked());
        assert!(!room.is_pinned());

        room.set_bookmark("Test".to_string(), "alice".to_string(), true, true, true);
        assert!(room.is_bookmarked());
        assert!(room.is_autojoin());
        assert!(room.is_pinned());
        assert_eq!(room.name, "Test");
        assert_eq!(room.self_nick().as_deref(), Some("alice"));

        room.clear_bookmark();
        assert!(!room.is_bookmarked());
        assert!(!room.is_pinned());
        assert!(!room.is_autojoin());
    }

    #[test]
    fn a_disconnect_asks_joined_rooms_to_rejoin_and_leaves_others_alone() {
        let mut room = room();
        room.mark_disconnected();
        assert!(room.take_rejoin_nick().is_none());

        room.join("alice", None).expect("join");
        room.mark_joined();
        room.mark_disconnected();
        assert!(room.needs_join());
        assert!(!room.is_joined());
        assert_eq!(room.take_rejoin_nick().as_deref(), Some("alice"));
        // The request is consumed once, so reconnecting does not loop.
        assert!(room.take_rejoin_nick().is_none());
    }
}
