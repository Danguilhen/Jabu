//! PubSub (Publish-Subscribe) management for blogging and PEP

use dioxus::prelude::{Readable, Signal, Writable};
use jid::Jid;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

use crate::{EventBus, Result, XmppEvent};

/// PubSub manager
pub struct PubSubManager {
    services: RwLock<HashMap<String, PubSubService>>,
    events: Arc<EventBus>,
}

impl PubSubManager {
    pub fn new(events: Arc<EventBus>) -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
            events,
        }
    }

    pub fn get_or_create(&self, jid: &Jid) -> PubSubService {
        let bare_jid = jid.to_bare().to_string();
        let mut services = self.services.write();
        services
            .entry(bare_jid.clone())
            .or_insert_with(|| PubSubService::new(bare_jid, self.events.clone()))
            .clone()
    }

    pub fn get(&self, jid: &Jid) -> Option<PubSubService> {
        let bare_jid = jid.to_bare().to_string();
        self.services.read().get(&bare_jid).cloned()
    }
}

/// PubSub service (per entity)
#[derive(Clone)]
pub struct PubSubService {
    pub jid: Jid,
    pub bare_jid: String,
    nodes: Arc<RwLock<HashMap<String, PubSubNode>>>,
    subscriptions: Arc<RwLock<HashMap<String, PubSubSubscription>>>,
    events: Arc<EventBus>,
    changed: Signal<u64>,
}

impl PubSubService {
    pub fn new(bare_jid: String, events: Arc<EventBus>) -> Self {
        let jid = bare_jid.parse().unwrap();
        Self {
            jid,
            bare_jid: bare_jid.clone(),
            nodes: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            events,
            changed: Signal::new(0),
        }
    }

    pub fn get_node(&self, node_id: &str) -> Option<PubSubNode> {
        self.nodes.read().get(node_id).cloned()
    }

    pub fn get_or_create_node(&self, node_id: String) -> PubSubNode {
        let mut nodes = self.nodes.write();
        nodes
            .entry(node_id.clone())
            .or_insert_with(|| PubSubNode::new(node_id, self.bare_jid.clone(), self.events.clone()))
            .clone()
    }

    pub fn nodes(&self) -> Vec<PubSubNode> {
        self.nodes.read().values().cloned().collect()
    }

    pub fn node_ids(&self) -> Vec<String> {
        self.nodes.read().keys().cloned().collect()
    }

    pub fn subscribe(&mut self, node_id: &str, jid: &Jid) -> Result<()> {
        let mut subs = self.subscriptions.write();
        let key = format!("{}@{}", node_id, jid.to_bare());
        subs.insert(
            key,
            PubSubSubscription::new(node_id.to_string(), jid.clone()),
        );
        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        Ok(())
    }

    pub fn unsubscribe(&mut self, node_id: &str, jid: &Jid) -> Result<()> {
        let mut subs = self.subscriptions.write();
        let key = format!("{}@{}", node_id, jid.to_bare());
        subs.remove(&key);
        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        Ok(())
    }

    pub fn is_subscribed(&self, node_id: &str, jid: &Jid) -> bool {
        let key = format!("{}@{}", node_id, jid.to_bare());
        self.subscriptions.read().contains_key(&key)
    }

    pub fn changed_signal(&self) -> Signal<u64> {
        self.changed.clone()
    }
}

/// PubSub node (e.g., blog, microblog, PEP node)
#[derive(Clone)]
pub struct PubSubNode {
    pub id: String,
    pub service_jid: String,
    pub config: PubSubNodeConfig,
    pub items: Arc<RwLock<Vec<PubSubItem>>>,
    pub subscribers: Arc<RwLock<Vec<Jid>>>,
    events: Arc<EventBus>,
    changed: Signal<u64>,
}

impl PubSubNode {
    pub fn new(id: String, service_jid: String, events: Arc<EventBus>) -> Self {
        Self {
            id,
            service_jid,
            config: PubSubNodeConfig::default(),
            items: Arc::new(RwLock::new(Vec::new())),
            subscribers: Arc::new(RwLock::new(Vec::new())),
            events,
            changed: Signal::new(0),
        }
    }

    pub fn items(&self) -> Vec<PubSubItem> {
        self.items.read().clone()
    }

    pub fn item_count(&self) -> usize {
        self.items.read().len()
    }

    pub fn get_item(&self, item_id: &str) -> Option<PubSubItem> {
        self.items.read().iter().find(|i| i.id == item_id).cloned()
    }

    pub fn add_item(&mut self, item: PubSubItem) {
        let mut items = self.items.write();
        items.insert(0, item); // Newest first

        // Keep only last 500 items
        if items.len() > 500 {
            items.truncate(500);
        }

        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
        self.emit_changed();
    }

    pub fn update_item(&mut self, item_id: &str, payload: serde_json::Value) -> bool {
        let mut items = self.items.write();
        if let Some(item) = items.iter_mut().find(|i| i.id == item_id) {
            item.payload = payload;
            item.updated_at = Some(chrono::Utc::now());
            let changed_val = *self.changed.read();
            self.changed.set(changed_val + 1);
            self.emit_changed();
            true
        } else {
            false
        }
    }

    pub fn add_subscriber(&mut self, jid: Jid) {
        let mut subs = self.subscribers.write();
        if !subs.iter().any(|j| j.to_bare() == jid.to_bare()) {
            subs.push(jid);
            let changed_val = *self.changed.read();
            self.changed.set(changed_val + 1);
        }
    }

    pub fn remove_subscriber(&mut self, jid: &Jid) {
        let mut subs = self.subscribers.write();
        subs.retain(|j| j.to_bare() != jid.to_bare());
        let changed_val = *self.changed.read();
        self.changed.set(changed_val + 1);
    }

    pub fn changed_signal(&self) -> Signal<u64> {
        self.changed.clone()
    }

    fn emit_changed(&self) {
        let _ = self.events.send(XmppEvent::PubSubNodeChanged {
            service_jid: self.service_jid.clone(),
            node_id: self.id.clone(),
        });
    }
}

/// PubSub node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubSubNodeConfig {
    pub node_type: PubSubNodeType,
    pub title: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub is_persistent: bool,
    pub is_public: bool,
    pub allow_subscribe: bool,
    pub allow_publish: PubSubPublishModel,
    pub max_items: Option<u32>,
    pub item_expire: Option<u64>, // seconds
    pub notify_retract: bool,
    pub notify_delete: bool,
    pub notify_config: bool,
    pub collection_nodes: Vec<String>, // For XEP-0248
}

impl Default for PubSubNodeConfig {
    fn default() -> Self {
        Self {
            node_type: PubSubNodeType::Leaf,
            title: None,
            description: None,
            language: Some("en".to_string()),
            is_persistent: true,
            is_public: true,
            allow_subscribe: true,
            allow_publish: PubSubPublishModel::Publishers,
            max_items: Some(100),
            item_expire: None,
            notify_retract: true,
            notify_delete: true,
            notify_config: true,
            collection_nodes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PubSubNodeType {
    Leaf,
    Collection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PubSubPublishModel {
    Publishers,
    Subscribers,
    Open,
}

/// PubSub item (blog post, microblog entry, PEP data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubSubItem {
    pub id: String,
    pub node_id: String,
    pub publisher: Jid,
    pub payload: serde_json::Value,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    // For blogging
    pub title: Option<String>,
    pub body: Option<String>,
    pub html: Option<String>,
    pub tags: Vec<String>,
    pub mentions: Vec<String>,
    pub media: Vec<MediaAttachment>,
    pub reactions: Vec<Reaction>,
    pub comments_count: u32,
    pub likes_count: u32,
    pub shares_count: u32,
}

impl PubSubItem {
    pub fn new(node_id: String, publisher: Jid, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            node_id,
            publisher,
            payload,
            published_at: chrono::Utc::now(),
            updated_at: None,
            expires_at: None,
            title: None,
            body: None,
            html: None,
            tags: Vec::new(),
            mentions: Vec::new(),
            media: Vec::new(),
            reactions: Vec::new(),
            comments_count: 0,
            likes_count: 0,
            shares_count: 0,
        }
    }

    pub fn from_blog_post(
        node_id: String,
        publisher: Jid,
        title: String,
        body: String,
        html: Option<String>,
        tags: Vec<String>,
        media: Vec<MediaAttachment>,
    ) -> Self {
        let mut item = Self::new(node_id, publisher, serde_json::json!({}));
        item.title = Some(title);
        item.body = Some(body);
        item.html = html;
        item.tags = tags;
        item.media = media;
        item
    }

    pub fn from_microblog(
        node_id: String,
        publisher: Jid,
        body: String,
        mentions: Vec<String>,
        media: Vec<MediaAttachment>,
    ) -> Self {
        let mut item = Self::new(node_id, publisher, serde_json::json!({}));
        item.body = Some(body);
        item.mentions = mentions;
        item.media = media;
        item
    }
}

/// PubSub subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubSubSubscription {
    pub node_id: String,
    pub jid: Jid,
    pub subscription_type: PubSubSubscriptionType,
    pub sub_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl PubSubSubscription {
    pub fn new(node_id: String, jid: Jid) -> Self {
        Self {
            node_id,
            jid,
            subscription_type: PubSubSubscriptionType::Items,
            sub_id: None,
            created_at: chrono::Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PubSubSubscriptionType {
    Items,
    Node,
}

/// Reaction to a PubSub item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    pub from_jid: Jid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Media attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAttachment {
    pub id: String,
    pub url: String,
    pub media_type: MediaType,
    pub name: Option<String>,
    pub size: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration: Option<f64>,
    pub thumbnail_url: Option<String>,
    pub blur_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Image,
    Video,
    Audio,
    File,
    Location,
    Contact,
}

/// Well-known PEP node names
pub mod pep_nodes {
    pub const AVATAR: &str = "urn:xmpp:avatar:metadata";
    pub const AVATAR_DATA: &str = "urn:xmpp:avatar:data";
    pub const V_CARD: &str = "urn:xmpp:vcard4";
    pub const MOOD: &str = "urn:xmpp:mood:0";
    pub const ACTIVITY: &str = "urn:xmpp:activity:0";
    pub const TUNE: &str = "urn:xmpp:tune:0";
    pub const GEOLOC: &str = "urn:xmpp:geoloc:0";
    pub const BOOKMARKS: &str = "storage:bookmarks";
    pub const BOOKMARKS2: &str = "urn:xmpp:bookmarks:2";
    pub const OMMEM_DEVICE_LIST: &str = "urn:xmpp:omemo:0:devicelist";
    pub const OMMEM_BUNDLES: &str = "urn:xmpp:omemo:0:bundles";
}

/// Well-known blogging node patterns
pub mod blog_nodes {
    pub const MICROBLOG: &str = "urn:xmpp:microblog:0";
    pub const BLOG: &str = "urn:xmpp:blog:0";
    pub const COMMENTS: &str = "urn:xmpp:comments:0";
}
