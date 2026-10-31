//! XEP-0472 community nodes and Movim-compatible community metadata.

use std::collections::HashMap;
use std::sync::Arc;

use jid::Jid;
use minidom::Element;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::change::ChangeCounter;

/// XEP-0472 social feed node type.
pub const NS_SOCIAL_FEED: &str = "urn:xmpp:pubsub-social-feed:1";
/// Movim gallery community node type.
pub const NS_SOCIAL_FEED_GALLERY: &str = "urn:xmpp:pubsub-social-feed:gallery:1";
/// Legacy Movim gallery type.
pub const NS_SOCIAL_GALLERY: &str = "urn:xmpp:pubsub-social-gallery:0";

/// Identity of one community: a `PubSub` leaf node on a service.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommunityId {
    pub service: Jid,
    pub node: String,
}

impl CommunityId {
    #[must_use]
    pub fn new(service: Jid, node: impl Into<String>) -> Self {
        Self {
            service,
            node: node.into(),
        }
    }

    #[must_use]
    pub fn parse(service: &str, node: &str) -> Option<Self> {
        Some(Self {
            service: service.parse().ok()?,
            node: node.to_string(),
        })
    }

    #[must_use]
    pub fn xmpp_uri(&self) -> String {
        format!("xmpp:{}?;node={}", self.service.to_bare(), self.node)
    }
}

/// Article feed or gallery-style community.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommunityKind {
    #[default]
    Articles,
    Gallery,
}

/// One disco#info identity advertised by a community node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityIdentity {
    pub category: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Metadata and local state for one community node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityInfo {
    pub id: CommunityId,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub kind: CommunityKind,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub identities: Vec<CommunityIdentity>,
    #[serde(default)]
    pub subscribers: Option<usize>,
    #[serde(default)]
    pub items: Option<usize>,
    #[serde(default)]
    pub publish_model: Option<String>,
    #[serde(default)]
    pub access_model: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub subscription_id: Option<String>,
    #[serde(default)]
    pub subscribed: bool,
    #[serde(default)]
    pub affiliation: Option<String>,
    #[serde(default)]
    pub notify: Option<String>,
    #[serde(default)]
    pub pinned: bool,
}

impl CommunityInfo {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(id: CommunityId) -> Self {
        Self {
            id,
            title: None,
            description: None,
            kind: CommunityKind::Articles,
            features: Vec::new(),
            identities: Vec::new(),
            subscribers: None,
            items: None,
            publish_model: None,
            access_model: None,
            avatar_url: None,
            subscription_id: None,
            subscribed: false,
            affiliation: None,
            notify: None,
            pinned: false,
        }
    }

    /// Read community metadata from a node `disco#info` query.
    #[must_use]
    pub fn from_disco(id: CommunityId, query: &Element) -> Option<Self> {
        let identities: Vec<CommunityIdentity> = query
            .children()
            .filter(|child| child.name() == "identity")
            .map(|identity| CommunityIdentity {
                category: identity.attr("category").unwrap_or_default().to_string(),
                type_name: identity.attr("type").unwrap_or_default().to_string(),
                lang: identity.attr("xml:lang").map(str::to_string),
                name: identity.attr("name").map(str::to_string),
            })
            .collect();
        let is_leaf = identities
            .iter()
            .any(|identity| identity.category == "pubsub" && identity.type_name == "leaf");
        if !is_leaf {
            return None;
        }
        let features: Vec<String> = query
            .children()
            .filter(|child| child.name() == "feature")
            .filter_map(|feature| feature.attr("var"))
            .map(str::to_string)
            .collect();
        let form = query
            .children()
            .find(|child| child.name() == "x" && child.ns() == "jabber:x:data");
        let title = form
            .and_then(|form| crate::form_value(form, "pubsub#title"))
            .filter(|value| !value.trim().is_empty());
        let description = form
            .and_then(|form| crate::form_value(form, "pubsub#description"))
            .filter(|value| !value.trim().is_empty());
        let node_type = form
            .and_then(|form| crate::form_value(form, "pubsub#type"))
            .unwrap_or_default();
        let kind = if node_type.contains("gallery") {
            CommunityKind::Gallery
        } else {
            CommunityKind::Articles
        };
        let subscribers = form
            .and_then(|form| crate::form_value(form, "pubsub#num_subscribers"))
            .and_then(|value| value.parse().ok());
        let items = form
            .and_then(|form| crate::form_value(form, "pubsub#num_items"))
            .and_then(|value| value.parse().ok());
        Some(Self {
            id,
            title,
            description,
            kind,
            features,
            identities,
            subscribers,
            items,
            publish_model: form.and_then(|form| crate::form_value(form, "pubsub#publish_model")),
            access_model: form.and_then(|form| crate::form_value(form, "pubsub#access_model")),
            avatar_url: None,
            subscription_id: None,
            subscribed: false,
            affiliation: None,
            notify: None,
            pinned: false,
        })
    }

    #[must_use]
    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|value| value == feature)
    }

    /// Whether this account may create posts on the node.
    #[must_use]
    pub fn can_publish(&self) -> bool {
        match self.publish_model.as_deref() {
            Some("open") => true,
            Some("subscribers") => self.subscribed,
            _ => matches!(
                self.affiliation.as_deref(),
                Some("owner" | "publisher" | "member")
            ),
        }
    }
}

/// Editable owner configuration for one community node.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityConfig {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub publish_model: Option<String>,
    #[serde(default)]
    pub access_model: Option<String>,
    #[serde(default)]
    pub kind: CommunityKind,
}

impl CommunityConfig {
    #[must_use]
    pub fn from_form(form: &Element) -> Self {
        let node_type = crate::form_value(form, "pubsub#type").unwrap_or_default();
        Self {
            title: crate::form_value(form, "pubsub#title"),
            description: crate::form_value(form, "pubsub#description"),
            publish_model: crate::form_value(form, "pubsub#publish_model"),
            access_model: crate::form_value(form, "pubsub#access_model"),
            kind: if node_type.contains("gallery") {
                CommunityKind::Gallery
            } else {
                CommunityKind::Articles
            },
        }
    }
}

/// Cached communities and their local follow state.
#[derive(Debug, Default)]
pub struct CommunityManager {
    communities: RwLock<HashMap<CommunityId, CommunityInfo>>,
    changed: Arc<ChangeCounter>,
}

impl CommunityManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&self) {
        self.communities.write().clear();
        self.changed.bump();
    }

    /// Insert metadata, preserving local state that the server does not send.
    #[allow(clippy::assigning_clones)]
    pub fn upsert(&self, mut info: CommunityInfo) {
        let mut communities = self.communities.write();
        if let Some(existing) = communities.get(&info.id) {
            info.subscribed |= existing.subscribed;
            if info.affiliation.is_none() {
                info.affiliation = existing.affiliation.clone();
            }
            if info.notify.is_none() {
                info.notify = existing.notify.clone();
            }
            if info.subscription_id.is_none() {
                info.subscription_id = existing.subscription_id.clone();
            }
            info.pinned |= existing.pinned;
            if info.avatar_url.is_none() {
                info.avatar_url = existing.avatar_url.clone();
            }
        }
        communities.insert(info.id.clone(), info);
        drop(communities);
        self.changed.bump();
    }

    #[must_use]
    pub fn get(&self, id: &CommunityId) -> Option<CommunityInfo> {
        self.communities.read().get(id).cloned()
    }

    #[must_use]
    pub fn list(&self) -> Vec<CommunityInfo> {
        let mut list: Vec<CommunityInfo> = self.communities.read().values().cloned().collect();
        list.sort_by(|left, right| {
            left.title
                .as_deref()
                .unwrap_or(&left.id.node)
                .cmp(right.title.as_deref().unwrap_or(&right.id.node))
        });
        list
    }

    #[must_use]
    pub fn followed(&self) -> Vec<CommunityInfo> {
        self.list()
            .into_iter()
            .filter(|community| community.subscribed)
            .collect()
    }

    pub fn set_subscribed(&self, id: &CommunityId, subscribed: bool) {
        if let Some(community) = self.communities.write().get_mut(id) {
            community.subscribed = subscribed;
            if !subscribed {
                community.subscription_id = None;
            }
            self.changed.bump();
        }
    }

    pub fn set_subscription_id(&self, id: &CommunityId, subid: Option<String>) {
        if let Some(community) = self.communities.write().get_mut(id) {
            community.subscription_id = subid;
            self.changed.bump();
        }
    }

    pub fn set_affiliation(&self, id: &CommunityId, affiliation: Option<String>) {
        if let Some(community) = self.communities.write().get_mut(id) {
            community.affiliation = affiliation;
            self.changed.bump();
        }
    }

    pub fn set_avatar(&self, id: &CommunityId, url: Option<String>) {
        if let Some(community) = self.communities.write().get_mut(id) {
            community.avatar_url = url;
            self.changed.bump();
        }
    }

    pub fn remove(&self, id: &CommunityId) {
        self.communities.write().remove(id);
        self.changed.bump();
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        self.changed.get()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn info_query(node_type: &str) -> Element {
        Element::from_str(&format!(
            r#"<query xmlns="http://jabber.org/protocol/disco#info"><identity category="pubsub" type="leaf"/><feature var="urn:xmpp:pubsub-social-feed:1"/><x xmlns="jabber:x:data" type="result"><field var="pubsub#title"><value>Rust</value></field><field var="pubsub#description"><value>Rust talks</value></field><field var="pubsub#type"><value>{node_type}</value></field><field var="pubsub#num_subscribers"><value>42</value></field><field var="pubsub#num_items"><value>7</value></field><field var="pubsub#publish_model"><value>open</value></field><field var="pubsub#access_model"><value>presence</value></field></x></query>"#
        ))
        .unwrap()
    }

    #[test]
    fn parses_article_community_metadata() {
        let id = CommunityId::parse("pubsub.example.org", "rust").unwrap();
        let info = CommunityInfo::from_disco(id.clone(), &info_query(NS_SOCIAL_FEED)).unwrap();
        assert_eq!(info.id, id);
        assert_eq!(info.title.as_deref(), Some("Rust"));
        assert_eq!(info.description.as_deref(), Some("Rust talks"));
        assert_eq!(info.kind, CommunityKind::Articles);
        assert_eq!(info.subscribers, Some(42));
        assert_eq!(info.items, Some(7));
        assert_eq!(info.publish_model.as_deref(), Some("open"));
        assert_eq!(info.access_model.as_deref(), Some("presence"));
        assert!(info.can_publish());
    }

    #[test]
    fn detects_gallery_communities() {
        let id = CommunityId::parse("pubsub.example.org", "photos").unwrap();
        let info = CommunityInfo::from_disco(id, &info_query(NS_SOCIAL_FEED_GALLERY)).unwrap();
        assert_eq!(info.kind, CommunityKind::Gallery);
    }

    #[test]
    fn manager_preserves_local_follow_state() {
        let manager = CommunityManager::new();
        let id = CommunityId::parse("pubsub.example.org", "rust").unwrap();
        manager.upsert(CommunityInfo::new(id.clone()));
        manager.set_subscribed(&id, true);
        manager.set_affiliation(&id, Some("owner".to_string()));
        let mut refreshed = CommunityInfo::new(id.clone());
        refreshed.title = Some("Rust".to_string());
        manager.upsert(refreshed);
        let info = manager.get(&id).unwrap();
        assert!(info.subscribed);
        assert_eq!(info.affiliation.as_deref(), Some("owner"));
        assert_eq!(info.title.as_deref(), Some("Rust"));
        assert_eq!(manager.followed().len(), 1);
    }

    #[test]
    fn parses_owner_config_forms() {
        let form = Element::from_str(
            r#"<x xmlns="jabber:x:data" type="submit"><field var="pubsub#title"><value>Rust</value></field><field var="pubsub#description"><value>Talks</value></field><field var="pubsub#publish_model"><value>subscribers</value></field><field var="pubsub#access_model"><value>open</value></field><field var="pubsub#type"><value>urn:xmpp:pubsub-social-feed:gallery:1</value></field></x>"#,
        )
        .unwrap();
        let config = CommunityConfig::from_form(&form);
        assert_eq!(config.title.as_deref(), Some("Rust"));
        assert_eq!(config.description.as_deref(), Some("Talks"));
        assert_eq!(config.publish_model.as_deref(), Some("subscribers"));
        assert_eq!(config.kind, CommunityKind::Gallery);
    }

    #[test]
    fn builds_a_movim_style_uri() {
        let id = CommunityId::parse("pubsub.example.org", "rust").unwrap();
        assert_eq!(id.xmpp_uri(), "xmpp:pubsub.example.org?;node=rust");
    }
}
