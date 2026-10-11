//! XEP-0313 archive paging state shared by the client and the event processor.

use std::collections::HashMap;

use parking_lot::{Mutex, RwLock};

/// A MAM query waiting for its `<fin>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MamQuery {
    /// Bare JID of the conversation or room.
    pub target: String,
    pub room: bool,
}

/// Tracks in-flight MAM queries and whether an archive has been fully paged.
#[derive(Debug, Default)]
pub struct MamManager {
    pending: Mutex<HashMap<String, MamQuery>>,
    complete: RwLock<HashMap<String, bool>>,
}

impl MamManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, query_id: &str, target: String, room: bool) {
        self.pending
            .lock()
            .insert(query_id.to_string(), MamQuery { target, room });
    }

    pub fn take(&self, query_id: &str) -> Option<MamQuery> {
        let query = self.pending.lock().remove(query_id)?;
        self.pending
            .lock()
            .retain(|_, candidate| candidate != &query);
        Some(query)
    }

    pub fn set_complete(&self, target: &str, complete: bool) {
        self.complete.write().insert(target.to_string(), complete);
    }

    pub fn is_complete(&self, target: &str) -> bool {
        self.complete.read().get(target).copied().unwrap_or(false)
    }

    pub fn pending_count(&self) -> usize {
        self.pending.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_queries_and_completion() {
        let mam = MamManager::new();
        mam.register("q1", "bob@example.org".into(), false);
        assert_eq!(mam.pending_count(), 1);

        let query = mam.take("q1").unwrap();
        assert_eq!(query.target, "bob@example.org");
        assert!(!query.room);
        assert_eq!(mam.pending_count(), 0);
        assert!(mam.take("q1").is_none());

        assert!(!mam.is_complete("bob@example.org"));
        mam.set_complete("bob@example.org", true);
        assert!(mam.is_complete("bob@example.org"));
    }
}
