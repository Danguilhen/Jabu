//! Storage abstraction for Jabu

pub mod redb_storage;
pub mod sqlite_storage;

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use xmpp_core::{ChatMessage, MucMessage, PubSubItem};

/// Storage backend trait
pub trait StorageBackend: Send + Sync {
    fn init(&self) -> Result<()>;
    fn close(&self) -> Result<()>;

    // Account methods
    fn save_account(&self, account: &StoredAccount) -> Result<()>;
    fn get_account(&self, jid: &str) -> Result<Option<StoredAccount>>;
    fn list_accounts(&self) -> Result<Vec<StoredAccount>>;
    fn delete_account(&self, jid: &str) -> Result<()>;

    // Contact methods
    fn save_contact(&self, contact: &StoredContact) -> Result<()>;
    fn get_contacts(&self, account_jid: &str) -> Result<Vec<StoredContact>>;
    fn delete_contact(&self, account_jid: &str, contact_jid: &str) -> Result<()>;

    // Chat methods
    fn save_chat(&self, chat: &StoredChat) -> Result<()>;
    fn get_chats(&self, account_jid: &str) -> Result<Vec<StoredChat>>;
    fn get_chat(&self, account_jid: &str, peer_jid: &str) -> Result<Option<StoredChat>> {
        Ok(self
            .get_chats(account_jid)?
            .into_iter()
            .find(|chat| chat.peer_jid == peer_jid))
    }

    // Call history
    fn save_call(&self, _call: &StoredCall) -> Result<()> {
        Ok(())
    }
    fn get_calls(&self, _account_jid: &str, _limit: usize) -> Result<Vec<StoredCall>> {
        Ok(Vec::new())
    }

    // Message methods
    fn save_message(&self, message: &ChatMessage) -> Result<()>;
    fn get_messages(
        &self,
        bare_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<ChatMessage>>;
    fn delete_message(&self, bare_jid: &str, message_id: &str) -> Result<()>;
    fn save_message_for(&self, _account_jid: &str, message: &ChatMessage) -> Result<()> {
        self.save_message(message)
    }
    fn get_messages_for(
        &self,
        _account_jid: &str,
        bare_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<ChatMessage>> {
        self.get_messages(bare_jid, limit, before_id)
    }
    fn search_messages(&self, _query: &str, _limit: usize) -> Result<Vec<ChatMessage>> {
        Ok(Vec::new())
    }
    fn search_messages_for(
        &self,
        _account_jid: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ChatMessage>> {
        self.search_messages(query, limit)
    }
    fn clear_messages_for(&self, _account_jid: &str, _bare_jid: &str) -> Result<()> {
        Ok(())
    }

    // MUC methods
    fn save_muc_room(&self, room: &StoredMucRoom) -> Result<()>;
    fn get_muc_rooms(&self, account_jid: &str) -> Result<Vec<StoredMucRoom>>;
    fn save_muc_message(&self, message: &MucMessage) -> Result<()>;
    fn get_muc_messages(
        &self,
        room_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<MucMessage>>;
    fn save_muc_message_for(&self, _account_jid: &str, message: &MucMessage) -> Result<()> {
        self.save_muc_message(message)
    }
    fn get_muc_messages_for(
        &self,
        _account_jid: &str,
        room_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<MucMessage>> {
        self.get_muc_messages(room_jid, limit, before_id)
    }
    fn search_muc_messages(&self, _query: &str, _limit: usize) -> Result<Vec<MucMessage>> {
        Ok(Vec::new())
    }
    fn search_muc_messages_for(
        &self,
        _account_jid: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MucMessage>> {
        self.search_muc_messages(query, limit)
    }
    fn clear_muc_messages_for(&self, _account_jid: &str, _room_jid: &str) -> Result<()> {
        Ok(())
    }

    // PubSub methods
    fn save_pubsub_node(&self, node: &StoredPubSubNode) -> Result<()>;
    fn get_pubsub_nodes(&self, service_jid: &str) -> Result<Vec<StoredPubSubNode>>;
    fn save_pubsub_item(&self, item: &PubSubItem) -> Result<()>;
    fn get_pubsub_items(&self, node_id: &str, limit: usize) -> Result<Vec<PubSubItem>>;
    fn save_pubsub_item_for(&self, _account_jid: &str, item: &PubSubItem) -> Result<()> {
        self.save_pubsub_item(item)
    }
    fn get_pubsub_items_for(
        &self,
        _account_jid: &str,
        node_id: &str,
        limit: usize,
    ) -> Result<Vec<PubSubItem>> {
        self.get_pubsub_items(node_id, limit)
    }

    // Spaces
    fn save_space(&self, _account_jid: &str, _space: &xmpp_core::Space) -> Result<()> {
        Ok(())
    }
    fn get_spaces(&self, _account_jid: &str) -> Result<Vec<xmpp_core::Space>> {
        Ok(Vec::new())
    }
    fn delete_space(&self, _account_jid: &str, _server: &str, _node: &str) -> Result<()> {
        Ok(())
    }

    // Settings
    fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()>;
    fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>>;
}

/// Settings key holding the saved account credentials.
pub const CREDENTIALS_KEY: &str = "account.credentials";

/// Account credentials persisted across launches.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredCredentials {
    pub jid: String,
    pub password: Option<String>,
    pub resource: Option<String>,
}

pub fn load_credentials(storage: &dyn StorageBackend) -> Option<StoredCredentials> {
    storage
        .get_setting(CREDENTIALS_KEY)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
}

pub fn save_credentials(
    storage: &dyn StorageBackend,
    credentials: &StoredCredentials,
) -> anyhow::Result<()> {
    storage.set_setting(CREDENTIALS_KEY, &serde_json::to_value(credentials)?)
}

/// Save credentials for one account, keyed by its bare JID.
pub fn save_account_credentials(
    storage: &dyn StorageBackend,
    credentials: &StoredCredentials,
) -> anyhow::Result<()> {
    let bare = credentials
        .jid
        .split('/')
        .next()
        .unwrap_or(&credentials.jid);
    storage.set_setting(
        &format!("credentials:{bare}"),
        &serde_json::to_value(credentials)?,
    )
}

/// Load credentials for one account.
pub fn load_account_credentials(
    storage: &dyn StorageBackend,
    jid: &str,
) -> Option<StoredCredentials> {
    let bare = jid.split('/').next().unwrap_or(jid);
    storage
        .get_setting(&format!("credentials:{bare}"))
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
}

/// Remove one account's saved credentials.
pub fn delete_account_credentials(storage: &dyn StorageBackend, jid: &str) -> anyhow::Result<()> {
    let bare = jid.split('/').next().unwrap_or(jid);
    storage.set_setting(&format!("credentials:{bare}"), &serde_json::Value::Null)
}

pub fn clear_credentials(storage: &dyn StorageBackend) -> anyhow::Result<()> {
    storage.set_setting(CREDENTIALS_KEY, &serde_json::Value::Null)
}

fn draft_key(account_jid: &str, peer_jid: &str) -> String {
    let account = account_jid.split('/').next().unwrap_or(account_jid);
    format!("draft:{account}:{peer_jid}")
}

/// Persist an unsent message for one conversation.
pub fn save_draft(
    storage: &dyn StorageBackend,
    account_jid: &str,
    peer_jid: &str,
    text: &str,
) -> anyhow::Result<()> {
    let value = if text.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(text.to_string())
    };
    storage.set_setting(&draft_key(account_jid, peer_jid), &value)
}

/// Load a conversation draft, if one was saved.
pub fn load_draft(
    storage: &dyn StorageBackend,
    account_jid: &str,
    peer_jid: &str,
) -> Option<String> {
    storage
        .get_setting(&draft_key(account_jid, peer_jid))
        .ok()
        .flatten()
        .and_then(|value| value.as_str().map(str::to_string))
}

/// Remove a conversation draft after it was sent.
pub fn clear_draft(
    storage: &dyn StorageBackend,
    account_jid: &str,
    peer_jid: &str,
) -> anyhow::Result<()> {
    storage.set_setting(&draft_key(account_jid, peer_jid), &serde_json::Value::Null)
}

/// Load one account's contacts, chats, rooms and microblog into a client.
pub fn load_session(
    storage: &dyn StorageBackend,
    client: &xmpp_core::XmppClient,
    account_jid: &str,
) {
    use xmpp_core::roster::RosterItem;

    let account = account_jid.split('/').next().unwrap_or(account_jid);

    for contact in storage.get_contacts(account).unwrap_or_default() {
        if let Ok(jid) = contact.jid.parse::<xmpp_core::Jid>() {
            let mut item = RosterItem::new(jid);
            item.name = contact.name;
            item.groups = contact.groups;
            item.subscription = contact.subscription;
            item.avatar_hash = contact.avatar_hash;
            item.is_blocked = contact.is_blocked;
            client.roster.upsert(item);
        }
    }

    for stored in storage.get_chats(account).unwrap_or_default() {
        if let Ok(jid) = stored.peer_jid.parse::<xmpp_core::Jid>() {
            let mut loaded = storage
                .get_messages_for(account, &stored.peer_jid, 500, None)
                .unwrap_or_default();
            if loaded.is_empty() {
                loaded = storage
                    .get_messages(&stored.peer_jid, 500, None)
                    .unwrap_or_default();
            }
            loaded.reverse();
            let mut chat = client.chats.get_or_create(&jid);
            chat.restore_preferences(xmpp_core::ChatPreferences {
                is_pinned: stored.is_pinned,
                is_archived: stored.is_archived,
                notifications_enabled: stored.notifications_enabled,
            });
            for message in loaded {
                chat.insert_loaded(message);
            }
        }
    }

    for room in storage.get_muc_rooms(account).unwrap_or_default() {
        if let Ok(jid) = room.jid.parse::<xmpp_core::Jid>() {
            client.mucs.get_or_create(&jid);
            client.mucs.restore_meta(&room.jid, room.name, room.nick);
            let mut loaded = storage
                .get_muc_messages_for(account, &room.jid, 500, None)
                .unwrap_or_default();
            if loaded.is_empty() {
                loaded = storage
                    .get_muc_messages(&room.jid, 500, None)
                    .unwrap_or_default();
            }
            loaded.reverse();
            for message in loaded {
                client.mucs.get_or_create(&jid).insert_loaded(message);
            }
        }
    }

    if let Ok(account_jid) = account.parse::<xmpp_core::Jid>() {
        let service = client
            .pubsub
            .get_or_create(&xmpp_core::Jid::from(account_jid.to_bare()));
        let mut node = service.get_or_create_node(xmpp_core::MICROBLOG_NODE.to_string());
        for item in storage
            .get_pubsub_items_for(account, xmpp_core::MICROBLOG_NODE, 50)
            .unwrap_or_default()
        {
            node.add_item(item);
        }
    }
}

/// OMEMO state persisted in the storage backend's settings table.
pub struct SettingsOmemoStore {
    storage: Arc<dyn StorageBackend>,
    key: String,
}

impl SettingsOmemoStore {
    pub fn new(storage: Arc<dyn StorageBackend>, jid: &str) -> Self {
        Self {
            storage,
            key: format!("omemo:{}", jid),
        }
    }
}

impl xmpp_core::omemo::OmemoStore for SettingsOmemoStore {
    fn load(&self) -> Option<String> {
        self.storage
            .get_setting(&self.key)
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(String::from))
    }

    fn save(&self, data: &str) -> xmpp_core::Result<()> {
        self.storage
            .set_setting(&self.key, &serde_json::Value::String(data.to_string()))
            .map_err(|e| xmpp_core::XmppError::StorageError(e.to_string()))
    }
}

/// Storage factory
pub struct StorageFactory;

impl StorageFactory {
    pub fn create(path: PathBuf) -> Result<Arc<dyn StorageBackend>> {
        let sqlite = sqlite_storage::SqliteStorage::new(path.clone())?;
        import_legacy_settings(&sqlite, &path);
        Ok(Arc::new(sqlite))
    }
}

fn import_legacy_settings(sqlite: &sqlite_storage::SqliteStorage, path: &std::path::Path) {
    let legacy = path.join("jabu.redb");
    if !legacy.exists() || sqlite.get_setting(CREDENTIALS_KEY).ok().flatten().is_some() {
        return;
    }
    let Ok(redb) = redb_storage::RedbStorage::new(path.to_path_buf()) else {
        return;
    };
    for key in [CREDENTIALS_KEY, "ui.notifications", "ui.timestamps"] {
        if let Ok(Some(value)) = redb.get_setting(key) {
            let _ = sqlite.set_setting(key, &value);
        }
    }
}

/// Stored account
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredAccount {
    pub jid: String,
    pub password_hash: Option<String>,
    pub resource: String,
    pub priority: i32,
    pub auto_reconnect: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_login: Option<chrono::DateTime<chrono::Utc>>,
}

/// Stored contact
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredContact {
    pub account_jid: String,
    pub jid: String,
    pub name: Option<String>,
    pub groups: Vec<String>,
    pub subscription: xmpp_core::roster::SubscriptionState,
    pub avatar_hash: Option<String>,
    pub is_blocked: bool,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Stored chat
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredChat {
    pub account_jid: String,
    pub peer_jid: String,
    pub unread_count: usize,
    pub last_message_id: Option<String>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub is_archived: bool,
    pub is_pinned: bool,
    pub notifications_enabled: bool,
}

/// A completed or missed call.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredCall {
    pub account_jid: String,
    pub id: String,
    pub peer_jid: String,
    pub direction: String,
    pub kind: String,
    pub state: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_secs: i64,
    pub reason: Option<String>,
}

/// Stored MUC room
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct StoredMucRoom {
    pub account_jid: String,
    pub jid: String,
    pub name: String,
    pub nick: String,
    pub is_joined: bool,
    pub is_favorite: bool,
    pub notifications_enabled: bool,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub space_server: Option<String>,
    pub space_node: Option<String>,
    pub directory_id: Option<String>,
    pub autojoin: bool,
    pub pinned: bool,
    pub weight: u32,
}

impl Default for StoredMucRoom {
    fn default() -> Self {
        Self {
            account_jid: String::new(),
            jid: String::new(),
            name: String::new(),
            nick: String::new(),
            is_joined: false,
            is_favorite: false,
            notifications_enabled: true,
            last_activity: chrono::Utc::now(),
            space_server: None,
            space_node: None,
            directory_id: None,
            autojoin: false,
            pinned: false,
            weight: 0,
        }
    }
}

/// Stored PubSub node
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredPubSubNode {
    pub service_jid: String,
    pub id: String,
    pub config: xmpp_core::pubsub::PubSubNodeConfig,
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
}

impl StorageBackend for Arc<dyn StorageBackend> {
    fn init(&self) -> Result<()> {
        self.as_ref().init()
    }
    fn close(&self) -> Result<()> {
        self.as_ref().close()
    }
    fn save_account(&self, account: &StoredAccount) -> Result<()> {
        self.as_ref().save_account(account)
    }
    fn get_account(&self, jid: &str) -> Result<Option<StoredAccount>> {
        self.as_ref().get_account(jid)
    }
    fn list_accounts(&self) -> Result<Vec<StoredAccount>> {
        self.as_ref().list_accounts()
    }
    fn delete_account(&self, jid: &str) -> Result<()> {
        self.as_ref().delete_account(jid)
    }
    fn save_contact(&self, contact: &StoredContact) -> Result<()> {
        self.as_ref().save_contact(contact)
    }
    fn get_contacts(&self, account_jid: &str) -> Result<Vec<StoredContact>> {
        self.as_ref().get_contacts(account_jid)
    }
    fn delete_contact(&self, account_jid: &str, contact_jid: &str) -> Result<()> {
        self.as_ref().delete_contact(account_jid, contact_jid)
    }
    fn save_chat(&self, chat: &StoredChat) -> Result<()> {
        self.as_ref().save_chat(chat)
    }
    fn get_chats(&self, account_jid: &str) -> Result<Vec<StoredChat>> {
        self.as_ref().get_chats(account_jid)
    }
    fn get_chat(&self, account_jid: &str, peer_jid: &str) -> Result<Option<StoredChat>> {
        self.as_ref().get_chat(account_jid, peer_jid)
    }
    fn save_call(&self, call: &StoredCall) -> Result<()> {
        self.as_ref().save_call(call)
    }
    fn get_calls(&self, account_jid: &str, limit: usize) -> Result<Vec<StoredCall>> {
        self.as_ref().get_calls(account_jid, limit)
    }
    fn save_message(&self, message: &ChatMessage) -> Result<()> {
        self.as_ref().save_message(message)
    }
    fn get_messages(
        &self,
        bare_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<ChatMessage>> {
        self.as_ref().get_messages(bare_jid, limit, before_id)
    }
    fn delete_message(&self, bare_jid: &str, message_id: &str) -> Result<()> {
        self.as_ref().delete_message(bare_jid, message_id)
    }
    fn save_message_for(&self, account_jid: &str, message: &ChatMessage) -> Result<()> {
        self.as_ref().save_message_for(account_jid, message)
    }
    fn get_messages_for(
        &self,
        account_jid: &str,
        bare_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<ChatMessage>> {
        self.as_ref()
            .get_messages_for(account_jid, bare_jid, limit, before_id)
    }
    fn search_messages(&self, query: &str, limit: usize) -> Result<Vec<ChatMessage>> {
        self.as_ref().search_messages(query, limit)
    }
    fn search_messages_for(
        &self,
        account_jid: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ChatMessage>> {
        self.as_ref().search_messages_for(account_jid, query, limit)
    }
    fn clear_messages_for(&self, account_jid: &str, bare_jid: &str) -> Result<()> {
        self.as_ref().clear_messages_for(account_jid, bare_jid)
    }
    fn save_muc_room(&self, room: &StoredMucRoom) -> Result<()> {
        self.as_ref().save_muc_room(room)
    }
    fn get_muc_rooms(&self, account_jid: &str) -> Result<Vec<StoredMucRoom>> {
        self.as_ref().get_muc_rooms(account_jid)
    }
    fn save_muc_message(&self, message: &MucMessage) -> Result<()> {
        self.as_ref().save_muc_message(message)
    }
    fn get_muc_messages(
        &self,
        room_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<MucMessage>> {
        self.as_ref().get_muc_messages(room_jid, limit, before_id)
    }
    fn save_muc_message_for(&self, account_jid: &str, message: &MucMessage) -> Result<()> {
        self.as_ref().save_muc_message_for(account_jid, message)
    }
    fn get_muc_messages_for(
        &self,
        account_jid: &str,
        room_jid: &str,
        limit: usize,
        before_id: Option<&str>,
    ) -> Result<Vec<MucMessage>> {
        self.as_ref()
            .get_muc_messages_for(account_jid, room_jid, limit, before_id)
    }
    fn search_muc_messages(&self, query: &str, limit: usize) -> Result<Vec<MucMessage>> {
        self.as_ref().search_muc_messages(query, limit)
    }
    fn search_muc_messages_for(
        &self,
        account_jid: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MucMessage>> {
        self.as_ref()
            .search_muc_messages_for(account_jid, query, limit)
    }
    fn clear_muc_messages_for(&self, account_jid: &str, room_jid: &str) -> Result<()> {
        self.as_ref().clear_muc_messages_for(account_jid, room_jid)
    }
    fn save_pubsub_node(&self, node: &StoredPubSubNode) -> Result<()> {
        self.as_ref().save_pubsub_node(node)
    }
    fn get_pubsub_nodes(&self, service_jid: &str) -> Result<Vec<StoredPubSubNode>> {
        self.as_ref().get_pubsub_nodes(service_jid)
    }
    fn save_pubsub_item(&self, item: &PubSubItem) -> Result<()> {
        self.as_ref().save_pubsub_item(item)
    }
    fn get_pubsub_items(&self, node_id: &str, limit: usize) -> Result<Vec<PubSubItem>> {
        self.as_ref().get_pubsub_items(node_id, limit)
    }
    fn save_pubsub_item_for(&self, account_jid: &str, item: &PubSubItem) -> Result<()> {
        self.as_ref().save_pubsub_item_for(account_jid, item)
    }
    fn get_pubsub_items_for(
        &self,
        account_jid: &str,
        node_id: &str,
        limit: usize,
    ) -> Result<Vec<PubSubItem>> {
        self.as_ref()
            .get_pubsub_items_for(account_jid, node_id, limit)
    }
    fn save_space(&self, account_jid: &str, space: &xmpp_core::Space) -> Result<()> {
        self.as_ref().save_space(account_jid, space)
    }
    fn get_spaces(&self, account_jid: &str) -> Result<Vec<xmpp_core::Space>> {
        self.as_ref().get_spaces(account_jid)
    }
    fn delete_space(&self, account_jid: &str, server: &str, node: &str) -> Result<()> {
        self.as_ref().delete_space(account_jid, server, node)
    }
    fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.as_ref().set_setting(key, value)
    }
    fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        self.as_ref().get_setting(key)
    }
}
