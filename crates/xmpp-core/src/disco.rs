//! Service discovery (XEP-0030) and HTTP file upload (XEP-0363) state.

use minidom::Element;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// XEP-0444 reaction restrictions advertised through a XEP-0128 form.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionRestrictions {
    #[serde(default)]
    pub max_reactions_per_user: Option<usize>,
    #[serde(default)]
    pub allowlist: Option<Vec<String>>,
}

impl ReactionRestrictions {
    #[must_use]
    pub fn allows(&self, emoji: &str) -> bool {
        self.allowlist
            .as_ref()
            .is_none_or(|allowed| allowed.iter().any(|item| item == emoji))
    }

    #[must_use]
    pub const fn max(&self) -> Option<usize> {
        self.max_reactions_per_user
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.max_reactions_per_user.is_none() && self.allowlist.is_none()
    }
}

/// Read a `urn:xmpp:reactions:0:restrictions` data form from a disco#info query.
#[must_use]
pub fn parse_reaction_restrictions(form: &Element) -> Option<ReactionRestrictions> {
    let mut form_type = None;
    let mut max_reactions_per_user = None;
    let mut allowlist = None;
    for field in form.children().filter(|child| child.name() == "field") {
        let Some(var) = field.attr("var") else {
            continue;
        };
        let values: Vec<String> = field
            .children()
            .filter(|child| child.name() == "value")
            .map(|value| value.text().trim().to_string())
            .filter(|value| !value.is_empty())
            .collect();
        match var {
            "FORM_TYPE" => form_type = values.first().cloned(),
            "max_reactions_per_user" => {
                max_reactions_per_user = values.first().and_then(|value| value.parse().ok());
            }
            "allowlist" => allowlist = (!values.is_empty()).then_some(values),
            _ => {}
        }
    }
    if form_type.as_deref() != Some("urn:xmpp:reactions:0:restrictions") {
        return None;
    }
    let restrictions = ReactionRestrictions {
        max_reactions_per_user,
        allowlist,
    };
    (!restrictions.is_empty()).then_some(restrictions)
}

/// A discovered XEP-0363 upload service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadService {
    pub jid: String,
    pub max_file_size: Option<u64>,
}

/// XEP-0157 contact addresses advertised by the server.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    #[serde(default)]
    pub abuse: Vec<String>,
    #[serde(default)]
    pub admin: Vec<String>,
    #[serde(default)]
    pub feedback: Vec<String>,
    #[serde(default)]
    pub sales: Vec<String>,
    #[serde(default)]
    pub security: Vec<String>,
    #[serde(default)]
    pub support: Vec<String>,
}

/// An external service (STUN/TURN) discovered through XEP-0215.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalService {
    #[serde(rename = "type")]
    pub kind: String,
    pub host: String,
    pub port: u16,
    pub transport: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub expires: Option<String>,
    pub restricted: bool,
}

/// A slot returned by an upload service: where to PUT and where the file ends up.
#[derive(Debug, Clone, PartialEq)]
pub struct UploadSlot {
    pub put_url: String,
    pub get_url: String,
    pub headers: Vec<(String, String)>,
}

/// Shared discovery state, updated by the event processor.
#[derive(Debug, Default)]
pub struct DiscoManager {
    server_features: RwLock<Vec<String>>,
    upload_service: RwLock<Option<UploadService>>,
    external_services: RwLock<Vec<ExternalService>>,
    server_info: RwLock<ServerInfo>,
    reaction_restrictions: RwLock<HashMap<String, ReactionRestrictions>>,
    spaces_service: RwLock<Option<String>>,
    pubsub_services: RwLock<Vec<String>>,
    pending_slots: Mutex<HashMap<String, tokio::sync::oneshot::Sender<UploadSlot>>>,
}

impl DiscoManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget everything learned about the previous account.
    pub fn clear(&self) {
        self.server_features.write().clear();
        *self.upload_service.write() = None;
        self.external_services.write().clear();
        *self.server_info.write() = ServerInfo::default();
        self.reaction_restrictions.write().clear();
        *self.spaces_service.write() = None;
        self.pubsub_services.write().clear();
    }

    /// The pubsub service used to host spaces (XEP-0503).
    pub fn set_spaces_service(&self, service: Option<String>) {
        *self.spaces_service.write() = service;
    }

    #[must_use]
    pub fn spaces_service(&self) -> Option<String> {
        self.spaces_service.read().clone()
    }

    /// Remember a pubsub service discovered on this server.
    pub fn add_pubsub_service(&self, service: String) {
        let mut services = self.pubsub_services.write();
        if !services.iter().any(|known| known == &service) {
            services.push(service);
        }
    }

    #[must_use]
    pub fn pubsub_services(&self) -> Vec<String> {
        self.pubsub_services.read().clone()
    }

    pub fn set_server_features(&self, features: Vec<String>) {
        *self.server_features.write() = features;
    }

    pub fn server_features(&self) -> Vec<String> {
        self.server_features.read().clone()
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        self.server_features.read().iter().any(|f| f == feature)
    }

    pub fn set_upload_service(&self, service: UploadService) {
        *self.upload_service.write() = Some(service);
    }

    pub fn upload_service(&self) -> Option<UploadService> {
        self.upload_service.read().clone()
    }

    pub fn set_external_services(&self, services: Vec<ExternalService>) {
        *self.external_services.write() = services;
    }

    pub fn external_services(&self) -> Vec<ExternalService> {
        self.external_services.read().clone()
    }

    pub fn set_server_info(&self, info: ServerInfo) {
        *self.server_info.write() = info;
    }

    #[must_use]
    pub fn server_info(&self) -> ServerInfo {
        self.server_info.read().clone()
    }

    pub fn set_reaction_restrictions(&self, jid: &str, restrictions: Option<ReactionRestrictions>) {
        let bare = jid.split('/').next().unwrap_or(jid).to_string();
        let mut map = self.reaction_restrictions.write();
        match restrictions {
            Some(restrictions) => {
                map.insert(bare, restrictions);
            }
            None => {
                map.remove(&bare);
            }
        }
    }

    #[must_use]
    pub fn reaction_restrictions(&self, jid: &str) -> Option<ReactionRestrictions> {
        let bare = jid.split('/').next().unwrap_or(jid);
        self.reaction_restrictions.read().get(bare).cloned()
    }

    #[must_use]
    pub fn stun_turn_servers(&self) -> Vec<ExternalService> {
        self.external_services
            .read()
            .iter()
            .filter(|service| service.kind == "stun" || service.kind == "turn")
            .cloned()
            .collect()
    }

    pub fn register_slot(&self, id: String, tx: tokio::sync::oneshot::Sender<UploadSlot>) {
        self.pending_slots.lock().insert(id, tx);
    }

    pub fn complete_slot(&self, id: &str, slot: UploadSlot) -> bool {
        match self.pending_slots.lock().remove(id) {
            Some(tx) => tx.send(slot).is_ok(),
            None => false,
        }
    }

    pub fn cancel_slot(&self, id: &str) {
        self.pending_slots.lock().remove(id);
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn parses_reaction_restrictions() {
        let xml = r#"<x xmlns="jabber:x:data" type="result"><field var="FORM_TYPE" type="hidden"><value>urn:xmpp:reactions:0:restrictions</value></field><field var="max_reactions_per_user"><value>1</value></field><field var="allowlist"><value>💘</value><value>❤️</value></field></x>"#;
        let form = Element::from_str(xml).unwrap();
        let restrictions = parse_reaction_restrictions(&form).unwrap();
        assert_eq!(restrictions.max(), Some(1));
        assert!(restrictions.allows("💘"));
        assert!(restrictions.allows("❤️"));
        assert!(!restrictions.allows("👍"));
    }

    #[test]
    fn igres_non_restriction_forms() {
        let xml = r#"<x xmlns="jabber:x:data" type="result"><field var="FORM_TYPE"><value>http://jabber.org/protocol/pubsub#meta-data</value></field></x>"#;
        let form = Element::from_str(xml).unwrap();
        assert!(parse_reaction_restrictions(&form).is_none());
    }

    #[test]
    fn remembers_spaces_and_pubsub_services() {
        let disco = DiscoManager::new();
        assert!(disco.spaces_service().is_none());
        disco.add_pubsub_service("pubsub.localhost".to_string());
        disco.add_pubsub_service("pubsub.localhost".to_string());
        disco.add_pubsub_service("pubsub.example.org".to_string());
        assert_eq!(
            disco.pubsub_services(),
            vec![
                "pubsub.localhost".to_string(),
                "pubsub.example.org".to_string()
            ]
        );
        disco.set_spaces_service(Some("pubsub.example.org".to_string()));
        assert_eq!(
            disco.spaces_service().as_deref(),
            Some("pubsub.example.org")
        );
        disco.clear();
        assert!(disco.spaces_service().is_none());
        assert!(disco.pubsub_services().is_empty());
    }
}
