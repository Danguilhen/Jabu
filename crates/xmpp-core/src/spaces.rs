use chrono::{DateTime, Utc};
use jid::Jid;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::{EventBus, XmppEvent};

pub const NS_SPACES: &str = "urn:xmpp:spaces:0";
pub const NS_SPACE_VOICE: &str = "urn:movim:spaces:voice:0";
pub const NS_SPACE_AVATAR: &str = "urn:xmpp:spaces:avatar:metadata:0";
pub const NS_SPACE_BANNER: &str = "urn:xmpp:spaces:banner:metadata:0";
pub const NS_SPACE_DIRECTORIES: &str = "https://slidge.im/spaces/bookmarks-hierarchy";
pub const NS_BOOKMARKS: &str = "urn:xmpp:bookmarks:1";
pub const NS_NOTIFICATIONS: &str = "urn:xmpp:notification-settings:0";
pub const NS_PINNED: &str = "urn:xmpp:bookmarks-pinning:0";

#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct SpaceId {
    pub server: String,
    pub node: String,
}

impl SpaceId {
    #[must_use]
    pub fn new(server: impl Into<String>, node: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            node: node.into(),
        }
    }

    #[must_use]
    pub fn key(&self) -> String {
        format!("{}:{}", self.server, self.node)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SpaceDirectory {
    pub id: String,
    pub name: String,
    pub order: usize,
}

/// Whether a room is a text chatroom or a Discord-style voice channel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpaceRoomKind {
    #[default]
    Text,
    Voice,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SpaceRoom {
    pub conference: String,
    pub name: String,
    #[serde(default)]
    pub kind: SpaceRoomKind,
    pub nick: Option<String>,
    pub autojoin: bool,
    pub directory_id: Option<String>,
    pub notify: SpaceNotify,
    pub pinned: bool,
    pub weight: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpaceNotify {
    #[default]
    Always,
    Mentions,
    Never,
}

impl SpaceNotify {
    pub const ALL: [Self; 3] = [Self::Always, Self::Mentions, Self::Never];

    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Mentions => "mentions",
            Self::Never => "never",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Always => "All messages",
            Self::Mentions => "Mentions only",
            Self::Never => "Nothing",
        }
    }

    #[must_use]
    pub fn from_slug(value: &str) -> Self {
        match value {
            "mentions" => Self::Mentions,
            "never" => Self::Never,
            _ => Self::Always,
        }
    }
}

/// Local mute state for a space.
///
/// A timed mute expires on its own; `Forever` lasts until the user unmutes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceMute {
    #[default]
    None,
    Until(DateTime<Utc>),
    Forever,
}

impl SpaceMute {
    #[must_use]
    pub fn is_muted(self, now: DateTime<Utc>) -> bool {
        match self {
            Self::None => false,
            Self::Forever => true,
            Self::Until(until) => until > now,
        }
    }

    #[must_use]
    pub fn until(self) -> Option<DateTime<Utc>> {
        match self {
            Self::Until(until) => Some(until),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpaceSubscription {
    #[default]
    None,
    Pending,
    Subscribed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpaceAffiliation {
    #[default]
    None,
    Member,
    Admin,
    Owner,
    Outcast,
}

impl SpaceAffiliation {
    #[must_use]
    pub fn from_str(value: &str) -> Self {
        match value {
            "owner" => Self::Owner,
            "admin" => Self::Admin,
            "member" => Self::Member,
            "outcast" => Self::Outcast,
            _ => Self::None,
        }
    }

    #[must_use]
    pub const fn is_manager(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SpaceMember {
    pub jid: String,
    pub affiliation: SpaceAffiliation,
    pub subscription: SpaceSubscription,
    pub sub_id: Option<String>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Space {
    pub id: SpaceId,
    pub name: String,
    pub description: String,
    pub avatar: Option<String>,
    #[serde(default)]
    pub banner: Option<String>,
    pub members: usize,
    pub unread: usize,
    pub pinned: bool,
    pub locked: bool,
    pub pending: bool,
    pub notify: SpaceNotify,
    #[serde(default)]
    pub mute: SpaceMute,
    /// Room JIDs muted locally on this account.
    #[serde(default)]
    pub muted_rooms: HashSet<String>,
    /// Whether the room list hides muted rooms.
    #[serde(default)]
    pub hide_muted_rooms: bool,
    pub subscription: SpaceSubscription,
    pub affiliation: SpaceAffiliation,
    pub directories: Vec<SpaceDirectory>,
    pub rooms: Vec<SpaceRoom>,
    pub subscriptions: Vec<SpaceMember>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub links: Vec<String>,
}

impl Space {
    #[must_use]
    pub fn first_room(&self) -> Option<&SpaceRoom> {
        self.rooms.first()
    }

    #[must_use]
    pub fn is_manager(&self) -> bool {
        self.affiliation.is_manager()
    }

    #[must_use]
    pub fn is_muted(&self) -> bool {
        self.mute.is_muted(Utc::now())
    }

    /// Whether one of this space's rooms is muted locally.
    #[must_use]
    pub fn is_room_muted(&self, conference: &str) -> bool {
        self.muted_rooms.contains(conference)
    }

    /// Whether a message from `conference` should produce a notification.
    #[must_use]
    pub fn allows_room_notification(
        &self,
        conference: &str,
        is_mention: bool,
        now: DateTime<Utc>,
    ) -> bool {
        !self.is_room_muted(conference) && self.allows_notification(is_mention, now)
    }

    /// Whether a room message in this space should produce a notification.
    #[must_use]
    pub fn allows_notification(&self, is_mention: bool, now: DateTime<Utc>) -> bool {
        if self.mute.is_muted(now) {
            return false;
        }
        match self.notify {
            SpaceNotify::Always => true,
            SpaceNotify::Mentions => is_mention,
            SpaceNotify::Never => false,
        }
    }

    #[must_use]
    pub fn pending_members(&self) -> Vec<SpaceMember> {
        self.subscriptions
            .iter()
            .filter(|member| member.subscription == SpaceSubscription::Pending)
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn room_groups(&self) -> Vec<(Option<SpaceDirectory>, Vec<SpaceRoom>)> {
        let mut groups = Vec::new();
        if self.directories.is_empty() {
            if !self.rooms.is_empty() {
                groups.push((None, self.rooms.clone()));
            }
            return groups;
        }

        let mut directories = self.directories.clone();
        directories.sort_by_key(|directory| directory.order);
        for directory in &directories {
            let rooms = self
                .rooms
                .iter()
                .filter(|room| room.directory_id.as_deref() == Some(directory.id.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            if !rooms.is_empty() {
                groups.push((Some(directory.clone()), rooms));
            }
        }
        let ungrouped = self
            .rooms
            .iter()
            .filter(|room| {
                room.directory_id
                    .as_ref()
                    .is_none_or(|id| directories.iter().all(|directory| &directory.id != id))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !ungrouped.is_empty() {
            groups.push((None, ungrouped));
        }
        groups
    }
}

pub struct SpaceManager {
    spaces: RwLock<HashMap<SpaceId, Space>>,
    known_nodes: RwLock<HashSet<SpaceId>>,
    events: Arc<EventBus>,
}

impl SpaceManager {
    #[must_use]
    pub fn new(events: Arc<EventBus>) -> Self {
        Self {
            spaces: RwLock::new(HashMap::new()),
            known_nodes: RwLock::new(HashSet::new()),
            events,
        }
    }

    pub fn clear(&self) {
        self.spaces.write().clear();
        self.known_nodes.write().clear();
        self.emit_changed();
    }

    #[must_use]
    pub fn list(&self) -> Vec<Space> {
        let mut spaces = self.spaces.read().values().cloned().collect::<Vec<_>>();
        spaces.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        spaces
    }

    #[must_use]
    pub fn get(&self, id: &SpaceId) -> Option<Space> {
        self.spaces.read().get(id).cloned()
    }

    #[must_use]
    pub fn get_by_parts(&self, server: &str, node: &str) -> Option<Space> {
        self.get(&SpaceId::new(server, node))
    }

    pub fn upsert(&self, space: Space) {
        self.known_nodes.write().insert(space.id.clone());
        self.spaces.write().insert(space.id.clone(), space);
        self.emit_changed();
    }

    pub fn remove(&self, id: &SpaceId) {
        self.spaces.write().remove(id);
        self.known_nodes.write().remove(id);
        self.emit_changed();
    }

    #[must_use]
    pub fn is_known_node(&self, server: &str, node: &str) -> bool {
        self.known_nodes
            .read()
            .contains(&SpaceId::new(server, node))
    }

    pub fn mark_known(&self, id: SpaceId) {
        self.known_nodes.write().insert(id);
    }

    pub fn set_metadata(
        &self,
        id: &SpaceId,
        name: Option<String>,
        description: Option<String>,
        avatar: Option<String>,
        members: Option<usize>,
        locked: Option<bool>,
    ) {
        let mut spaces = self.spaces.write();
        let space = spaces.entry(id.clone()).or_insert_with(|| Space {
            id: id.clone(),
            name: id.node.clone(),
            ..Space::default()
        });
        if let Some(value) = name.filter(|value| !value.is_empty()) {
            space.name = value;
        }
        if let Some(value) = description {
            space.description = value;
        }
        if avatar.is_some() {
            space.avatar = avatar;
        }
        if let Some(value) = members {
            space.members = value;
        }
        if let Some(value) = locked {
            space.locked = value;
        }
        self.known_nodes.write().insert(id.clone());
        drop(spaces);
        self.emit_changed();
    }

    /// Set the space banner URL.
    pub fn set_banner(&self, id: &SpaceId, banner: Option<String>) {
        let mut spaces = self.spaces.write();
        let space = spaces.entry(id.clone()).or_insert_with(|| Space {
            id: id.clone(),
            name: id.node.clone(),
            ..Space::default()
        });
        space.banner = banner;
        drop(spaces);
        self.emit_changed();
    }

    /// Set the XEP-0496 parent and linked nodes for a space.
    pub fn set_relationships(
        &self,
        id: &SpaceId,
        relationships: &crate::pubsub::relationships::NodeRelationships,
    ) {
        let mut spaces = self.spaces.write();
        let space = spaces.entry(id.clone()).or_insert_with(|| Space {
            id: id.clone(),
            name: id.node.clone(),
            ..Space::default()
        });
        space.parent.clone_from(&relationships.parent);
        space.links.clone_from(&relationships.links);
        drop(spaces);
        self.emit_changed();
    }

    pub fn set_rooms(&self, id: &SpaceId, rooms: Vec<SpaceRoom>) {
        let mut spaces = self.spaces.write();
        let space = spaces.entry(id.clone()).or_insert_with(|| Space {
            id: id.clone(),
            name: id.node.clone(),
            ..Space::default()
        });
        space.rooms = rooms;
        self.known_nodes.write().insert(id.clone());
        drop(spaces);
        self.emit_changed();
    }

    /// Apply one node update to a space's room list.
    ///
    /// A pubsub event carries only the items that changed, so replacing the
    /// list with it would drop every other room. Rooms named in `updates` are
    /// replaced by conference address, `retracted` are removed, the rest is
    /// kept as it was.
    pub fn merge_rooms(&self, id: &SpaceId, updates: Vec<SpaceRoom>, retracted: &[String]) {
        let mut spaces = self.spaces.write();
        let space = spaces.entry(id.clone()).or_insert_with(|| Space {
            id: id.clone(),
            name: id.node.clone(),
            ..Space::default()
        });
        space.rooms.retain(|room| {
            !retracted.contains(&room.conference)
                && !updates
                    .iter()
                    .any(|update| update.conference == room.conference)
        });
        space.rooms.extend(updates);
        space.rooms.sort_by_key(|room| room.weight);
        self.known_nodes.write().insert(id.clone());
        drop(spaces);
        self.emit_changed();
    }

    pub fn set_directories(&self, id: &SpaceId, directories: Vec<SpaceDirectory>) {
        let mut spaces = self.spaces.write();
        let space = spaces.entry(id.clone()).or_insert_with(|| Space {
            id: id.clone(),
            name: id.node.clone(),
            ..Space::default()
        });
        space.directories = directories;
        self.known_nodes.write().insert(id.clone());
        drop(spaces);
        self.emit_changed();
    }

    pub fn set_subscription(&self, id: &SpaceId, subscription: SpaceSubscription) {
        let mut spaces = self.spaces.write();
        let space = spaces.entry(id.clone()).or_insert_with(|| Space {
            id: id.clone(),
            name: id.node.clone(),
            ..Space::default()
        });
        space.subscription = subscription;
        space.pending = subscription == SpaceSubscription::Pending;
        self.known_nodes.write().insert(id.clone());
        drop(spaces);
        self.emit_changed();
    }

    pub fn set_affiliation(&self, id: &SpaceId, affiliation: SpaceAffiliation) {
        let mut spaces = self.spaces.write();
        if let Some(space) = spaces.get_mut(id) {
            space.affiliation = affiliation;
        }
        drop(spaces);
        self.emit_changed();
    }

    pub fn set_subscriptions(&self, id: &SpaceId, members: Vec<SpaceMember>) {
        let mut spaces = self.spaces.write();
        if let Some(space) = spaces.get_mut(id) {
            space.subscriptions = members;
        }
        drop(spaces);
        self.emit_changed();
    }

    pub fn set_notify(&self, id: &SpaceId, notify: SpaceNotify) {
        let mut spaces = self.spaces.write();
        if let Some(space) = spaces.get_mut(id) {
            space.notify = notify;
        }
        drop(spaces);
        self.emit_changed();
    }

    pub fn set_mute(&self, id: &SpaceId, mute: SpaceMute) {
        let mut spaces = self.spaces.write();
        if let Some(space) = spaces.get_mut(id) {
            space.mute = mute;
        }
        drop(spaces);
        self.emit_changed();
    }

    /// Mute or unmute one room of a space locally.
    pub fn set_room_muted(&self, id: &SpaceId, conference: &str, muted: bool) {
        let mut spaces = self.spaces.write();
        if let Some(space) = spaces.get_mut(id) {
            if muted {
                space.muted_rooms.insert(conference.to_string());
            } else {
                space.muted_rooms.remove(conference);
            }
        }
        drop(spaces);
        self.emit_changed();
    }

    /// Hide or show muted rooms in a space's sidebar locally.
    pub fn set_hide_muted_rooms(&self, id: &SpaceId, hide: bool) {
        let mut spaces = self.spaces.write();
        if let Some(space) = spaces.get_mut(id) {
            space.hide_muted_rooms = hide;
        }
        drop(spaces);
        self.emit_changed();
    }

    pub fn set_pinned(&self, id: &SpaceId, pinned: bool) {
        let mut spaces = self.spaces.write();
        if let Some(space) = spaces.get_mut(id) {
            space.pinned = pinned;
        }
        drop(spaces);
        self.emit_changed();
    }

    pub fn set_unread(&self, id: &SpaceId, unread: usize) {
        let mut spaces = self.spaces.write();
        if let Some(space) = spaces.get_mut(id) {
            space.unread = unread;
        }
        drop(spaces);
    }

    pub fn set_room_notify(&self, id: &SpaceId, conference: &str, notify: SpaceNotify) {
        let mut spaces = self.spaces.write();
        if let Some(space) = spaces.get_mut(id) {
            if let Some(room) = space
                .rooms
                .iter_mut()
                .find(|room| room.conference == conference)
            {
                room.notify = notify;
            }
        }
        drop(spaces);
        self.emit_changed();
    }

    fn emit_changed(&self) {
        let _ = self.events.send(XmppEvent::SpacesChanged);
    }
}

impl Default for SpaceManager {
    fn default() -> Self {
        Self::new(Arc::new(EventBus::new()))
    }
}

#[must_use]
pub fn parse_space_uri(value: &str) -> Option<SpaceId> {
    let trimmed = value.trim().trim_start_matches("xmpp:");
    let (server, rest) = trimmed
        .split_once("?;")
        .or_else(|| trimmed.split_once('?'))?;
    let node = rest.strip_prefix("node=")?;
    if server.is_empty() || node.is_empty() {
        return None;
    }
    Some(SpaceId::new(server, node))
}

#[must_use]
pub fn space_jid(server: &str) -> Option<Jid> {
    server.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_movim_space_uri() {
        let id = parse_space_uri("xmpp:spaces.example.org?;node=dev").unwrap();
        assert_eq!(id.server, "spaces.example.org");
        assert_eq!(id.node, "dev");
    }

    #[test]
    fn rejects_incomplete_space_uri() {
        assert!(parse_space_uri("xmpp:spaces.example.org").is_none());
        assert!(parse_space_uri("?;node=dev").is_none());
    }

    #[test]
    fn stores_node_relationships() {
        let spaces = SpaceManager::default();
        let id = SpaceId::new("pubsub.example.org", "node");
        spaces.set_relationships(
            &id,
            &crate::pubsub::relationships::NodeRelationships {
                parent: Some("urn:xmpp:microblog:0".to_string()),
                links: vec!["urn:xmpp:microblog:0:comments/a".to_string()],
            },
        );
        let space = spaces.get(&id).expect("space");
        assert_eq!(space.parent.as_deref(), Some("urn:xmpp:microblog:0"));
        assert_eq!(
            space.links,
            vec!["urn:xmpp:microblog:0:comments/a".to_string()]
        );
    }

    fn room(conference: &str) -> SpaceRoom {
        SpaceRoom {
            conference: conference.to_string(),
            ..SpaceRoom::default()
        }
    }

    #[test]
    fn merging_a_node_update_keeps_the_other_rooms() {
        let spaces = SpaceManager::default();
        let id = SpaceId::new("pubsub.example.org", "node");
        spaces.set_rooms(
            &id,
            vec![
                room("a@conference.example.org"),
                room("b@conference.example.org"),
            ],
        );

        spaces.merge_rooms(&id, vec![room("c@conference.example.org")], &[]);

        let conferences: Vec<String> = spaces
            .get(&id)
            .unwrap()
            .rooms
            .into_iter()
            .map(|room| room.conference)
            .collect();
        assert_eq!(
            conferences,
            vec![
                "a@conference.example.org",
                "b@conference.example.org",
                "c@conference.example.org"
            ]
        );
    }

    #[test]
    fn merging_an_update_replaces_and_retracts() {
        let spaces = SpaceManager::default();
        let id = SpaceId::new("pubsub.example.org", "node");
        spaces.set_rooms(
            &id,
            vec![
                room("a@conference.example.org"),
                room("b@conference.example.org"),
            ],
        );
        let mut renamed = room("a@conference.example.org");
        renamed.name = "Renamed".to_string();

        spaces.merge_rooms(
            &id,
            vec![renamed],
            &["b@conference.example.org".to_string()],
        );

        let rooms = spaces.get(&id).unwrap().rooms;
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].conference, "a@conference.example.org");
        assert_eq!(rooms[0].name, "Renamed");
    }

    #[test]
    fn timed_mutes_expire() {
        let now = Utc::now();
        assert!(!SpaceMute::None.is_muted(now));
        assert!(SpaceMute::Forever.is_muted(now));
        assert!(SpaceMute::Until(now + chrono::Duration::minutes(15)).is_muted(now));
        assert!(!SpaceMute::Until(now - chrono::Duration::minutes(1)).is_muted(now));
    }

    #[test]
    fn notification_gating_honours_mute_and_mode() {
        let now = Utc::now();
        let mut space = Space {
            notify: SpaceNotify::Mentions,
            ..Space::default()
        };
        assert!(!space.allows_notification(false, now));
        assert!(space.allows_notification(true, now));

        space.notify = SpaceNotify::Always;
        assert!(space.allows_notification(false, now));

        space.notify = SpaceNotify::Never;
        assert!(!space.allows_notification(true, now));

        space.notify = SpaceNotify::Always;
        space.mute = SpaceMute::Until(now + chrono::Duration::hours(1));
        assert!(!space.allows_notification(true, now));
    }

    #[test]
    fn a_missing_mute_field_still_deserializes() {
        let json = serde_json::json!({
            "id": { "server": "spaces.example.org", "node": "dev" },
            "name": "Developers",
            "description": "",
            "avatar": null,
            "members": 0,
            "unread": 0,
            "pinned": false,
            "locked": false,
            "pending": false,
            "notify": "always",
            "subscription": "none",
            "affiliation": "none",
            "directories": [],
            "rooms": [],
            "subscriptions": []
        });
        let space: Space = serde_json::from_value(json).expect("space without mute");
        assert_eq!(space.mute, SpaceMute::None);
        assert!(space.muted_rooms.is_empty());
        assert!(!space.hide_muted_rooms);
    }

    #[test]
    fn muting_a_room_suppresses_its_notifications() {
        let now = Utc::now();
        let mut space = Space {
            notify: SpaceNotify::Always,
            ..Space::default()
        };
        space
            .muted_rooms
            .insert("a@conference.example.org".to_string());

        assert!(space.is_room_muted("a@conference.example.org"));
        assert!(!space.allows_room_notification("a@conference.example.org", true, now));
        assert!(space.allows_room_notification("b@conference.example.org", false, now));
    }

    #[test]
    fn room_mutes_survive_a_room_refresh() {
        let spaces = SpaceManager::default();
        let id = SpaceId::new("pubsub.example.org", "node");
        spaces.set_rooms(&id, vec![room("a@conference.example.org")]);
        spaces.set_room_muted(&id, "a@conference.example.org", true);

        spaces.set_rooms(&id, vec![room("a@conference.example.org")]);

        assert!(spaces
            .get(&id)
            .unwrap()
            .is_room_muted("a@conference.example.org"));
    }

    #[test]
    fn unmuting_a_room_removes_the_entry() {
        let spaces = SpaceManager::default();
        let id = SpaceId::new("pubsub.example.org", "node");
        spaces.set_metadata(&id, Some("Space".to_string()), None, None, None, None);

        spaces.set_room_muted(&id, "a@conference.example.org", true);
        spaces.set_room_muted(&id, "a@conference.example.org", false);

        assert!(!spaces
            .get(&id)
            .unwrap()
            .is_room_muted("a@conference.example.org"));
    }

    #[test]
    fn setting_a_mute_updates_the_space() {
        let spaces = SpaceManager::default();
        let id = SpaceId::new("pubsub.example.org", "node");
        spaces.set_metadata(&id, Some("Space".to_string()), None, None, None, None);
        spaces.set_mute(&id, SpaceMute::Forever);
        assert_eq!(spaces.get(&id).unwrap().mute, SpaceMute::Forever);
        spaces.set_mute(&id, SpaceMute::None);
        assert_eq!(spaces.get(&id).unwrap().mute, SpaceMute::None);
    }
}
