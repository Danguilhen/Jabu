//! XEP-0198 stream management state, counters and unacked stanza queue.

use minidom::Element;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const MAX_UNACKED: usize = 2048;

/// Details from the server's `<enabled/>` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmEnabled {
    pub id: Option<String>,
    pub resume: bool,
    pub max: Option<u32>,
}

/// Tracks protocol counters and unacknowledged stanzas for XEP-0198.
///
/// `handled` counts inbound stanzas we have processed (reported in `<a h>`),
/// `sent` counts outbound stanzas, and `acked` is the last `h` the server
/// acknowledged. `pending` holds the outbound stanzas that are not acknowledged
/// yet so they can be retransmitted after a resume.
#[derive(Debug, Default)]
pub struct StreamManagement {
    enabled: RwLock<Option<SmEnabled>>,
    handled: AtomicU64,
    sent: AtomicU64,
    acked: AtomicU64,
    pending: Mutex<VecDeque<Element>>,
    resumed: AtomicBool,
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

    /// Reset all counters, state and the unacked queue for a fresh stream.
    pub fn reset(&self) {
        *self.enabled.write() = None;
        self.handled.store(0, Ordering::SeqCst);
        self.sent.store(0, Ordering::SeqCst);
        self.acked.store(0, Ordering::SeqCst);
        self.pending.lock().clear();
        self.resumed.store(false, Ordering::SeqCst);
    }

    /// Keep the resumption id and unacked queue so the next stream can resume.
    ///
    /// Does nothing when stream management was never enabled.
    pub fn connection_lost(&self) {
        if !self.is_enabled() {
            self.reset();
        }
    }

    pub fn clear_resumption(&self) {
        self.reset();
    }

    pub fn note_handled(&self) -> u64 {
        self.handled.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn note_sent(&self) -> u64 {
        self.sent.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Count an outbound stanza and remember it until the server acknowledges it.
    pub fn note_sent_stanza(&self, stanza: Element) -> u64 {
        if !is_countable(&stanza) {
            return self.sent();
        }
        let count = self.note_sent();
        let mut pending = self.pending.lock();
        if pending.len() >= MAX_UNACKED {
            pending.pop_front();
        }
        pending.push_back(stanza);
        count
    }

    pub fn handled(&self) -> u64 {
        self.handled.load(Ordering::SeqCst)
    }

    pub fn sent(&self) -> u64 {
        self.sent.load(Ordering::SeqCst)
    }

    /// Record an acknowledgement and drop the stanzas it covers.
    pub fn set_acked(&self, h: u64) {
        let previous = self.acked.swap(h, Ordering::SeqCst);
        let newly_acked = h.saturating_sub(previous) as usize;
        if newly_acked > 0 {
            let mut pending = self.pending.lock();
            let drain = newly_acked.min(pending.len());
            pending.drain(..drain);
        }
    }

    pub fn acked(&self) -> u64 {
        self.acked.load(Ordering::SeqCst)
    }

    /// Outbound stanzas not yet acknowledged by the server.
    pub fn unacked(&self) -> u64 {
        self.sent().saturating_sub(self.acked())
    }

    pub fn unacked_stanzas(&self) -> Vec<Element> {
        self.pending.lock().iter().cloned().collect()
    }

    pub fn take_unacked_stanzas(&self) -> Vec<Element> {
        std::mem::take(&mut *self.pending.lock())
            .into_iter()
            .collect()
    }

    /// Details needed for `<resume previd h/>`, when the server allows it.
    pub fn resume_request(&self) -> Option<(String, u64)> {
        let details = self.details()?;
        if !details.resume {
            return None;
        }
        Some((details.id?, self.handled()))
    }

    pub fn resume_ok(&self, h: u64) {
        self.set_acked(h);
        self.resumed.store(true, Ordering::SeqCst);
    }

    pub fn was_resumed(&self) -> bool {
        self.resumed.load(Ordering::SeqCst)
    }

    /// Read and clear the flag so a reconnect only skips the reset once.
    pub fn take_resumed(&self) -> bool {
        self.resumed.swap(false, Ordering::SeqCst)
    }
}

fn is_countable(stanza: &Element) -> bool {
    matches!(stanza.name(), "message" | "presence" | "iq")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str) -> Element {
        Element::builder("message", "jabber:client")
            .attr("id", id)
            .build()
    }

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

    #[test]
    fn acknowledges_only_covering_stanzas() {
        let sm = StreamManagement::new();
        sm.enable(Some("abc".into()), true, None);
        sm.note_sent_stanza(message("one"));
        sm.note_sent_stanza(Element::builder("r", "urn:xmpp:sm:3").build());
        sm.note_sent_stanza(message("two"));

        assert_eq!(sm.sent(), 2);
        assert_eq!(sm.unacked_stanzas().len(), 2);

        sm.resume_ok(1);
        assert!(sm.was_resumed());
        let left = sm.unacked_stanzas();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].attr("id"), Some("two"));

        assert_eq!(sm.take_unacked_stanzas().len(), 1);
        assert!(sm.unacked_stanzas().is_empty());
        assert!(sm.take_resumed());
        assert!(!sm.take_resumed());
    }

    #[test]
    fn lost_connection_keeps_resumption_state() {
        let sm = StreamManagement::new();
        sm.note_sent_stanza(message("one"));
        sm.connection_lost();
        assert!(!sm.is_enabled());
        assert!(sm.unacked_stanzas().is_empty());

        sm.enable(Some("id".into()), true, None);
        sm.note_sent_stanza(message("two"));
        sm.connection_lost();
        assert!(sm.is_enabled());
        assert_eq!(sm.resume_request(), Some(("id".into(), 0)));
        assert_eq!(sm.unacked_stanzas().len(), 1);
    }
}
