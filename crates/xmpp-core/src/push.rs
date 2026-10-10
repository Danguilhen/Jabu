//! XEP-0357 push notification registration state.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// The push service currently enabled for this account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushRegistration {
    pub service: String,
    pub node: String,
}

/// Shared push registration state, updated by the event processor.
#[derive(Debug, Default)]
pub struct PushManager {
    current: RwLock<Option<PushRegistration>>,
}

impl PushManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_enabled(&self, service: String, node: String) {
        *self.current.write() = Some(PushRegistration { service, node });
    }

    pub fn clear(&self) {
        *self.current.write() = None;
    }

    pub fn current(&self) -> Option<PushRegistration> {
        self.current.read().clone()
    }

    pub fn is_enabled(&self) -> bool {
        self.current.read().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_registration() {
        let manager = PushManager::new();
        assert!(!manager.is_enabled());
        manager.set_enabled("push.example.com".into(), "posts".into());
        assert!(manager.is_enabled());
        assert_eq!(
            manager.current(),
            Some(PushRegistration {
                service: "push.example.com".into(),
                node: "posts".into()
            })
        );
        manager.clear();
        assert!(!manager.is_enabled());
    }
}
