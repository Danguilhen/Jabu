//! Service discovery (XEP-0030) and HTTP file upload (XEP-0363) state.

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
