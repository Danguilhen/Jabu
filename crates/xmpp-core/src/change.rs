use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct ChangeCounter {
    value: AtomicU64,
}

impl ChangeCounter {
    pub fn bump(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}
