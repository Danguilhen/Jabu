//! XEP-0198 stream management state and counters.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Details from the server's `<enabled/>` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmEnabled {
    pub id: Option<String>,
    pub resume: bool,
    pub max: Option<u32>,
}

/// Tracks protocol counters for XEP-0198.
///
/// `handled` counts inbound stanzas we have processed (reported in `<a h>`),
/// `sent` counts outbound stanzas, and `acked` is the last `h` the server
/// acknowledged.
#[derive(Debug, Default)]
pub struct StreamManagement {
    enabled: RwLock<Option<SmEnabled>>,
    handled: AtomicU64,
    sent: AtomicU64,
    acked: AtomicU64,
}

impl StreamManagement {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enable(&self, id: Option<String>, resume: bool, max: Option<u32>) {
        *self.enabled.write() = Some(SmEnabled { id, resume, max });
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.read().is_some()
    }

    pub fn details(&self) -> Option<SmEnabled> {
        self.enabled.read().clone()
    }

    /// Reset all counters and state for a new stream.
    pub fn reset(&self) {
        *self.enabled.write() = None;
        self.handled.store(0, Ordering::SeqCst);
        self.sent.store(0, Ordering::SeqCst);
        self.acked.store(0, Ordering::SeqCst);
    }

    pub fn note_handled(&self) -> u64 {
        self.handled.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn note_sent(&self) -> u64 {
        self.sent.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn handled(&self) -> u64 {
        self.handled.load(Ordering::SeqCst)
    }

    pub fn sent(&self) -> u64 {
        self.sent.load(Ordering::SeqCst)
    }

    pub fn set_acked(&self, h: u64) {
        self.acked.store(h, Ordering::SeqCst);
    }

    pub fn acked(&self) -> u64 {
        self.acked.load(Ordering::SeqCst)
    }

    /// Outbound stanzas not yet acknowledged by the server.
    pub fn unacked(&self) -> u64 {
        self.sent().saturating_sub(self.acked())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_resets() {
        let sm = StreamManagement::new();
        sm.enable(Some("abc".into()), false, Some(300));
        assert!(sm.is_enabled());
        assert_eq!(sm.details().unwrap().id.as_deref(), Some("abc"));

        assert_eq!(sm.note_handled(), 1);
        assert_eq!(sm.note_handled(), 2);
        assert_eq!(sm.handled(), 2);
        assert_eq!(sm.note_sent(), 1);
        sm.set_acked(1);
        assert_eq!(sm.acked(), 1);
        assert_eq!(sm.unacked(), 0);

        sm.reset();
        assert!(!sm.is_enabled());
        assert_eq!(sm.handled(), 0);
        assert_eq!(sm.sent(), 0);
        assert_eq!(sm.acked(), 0);
    }
}
