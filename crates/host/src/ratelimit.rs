//! Simple per-client fixed-window rate limiter for public routes.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// Fixed-window request counter keyed by client address.
pub struct RateLimiter {
    limit: u32,
    window: Duration,
    entries: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    /// `limit == 0` disables the limiter.
    #[must_use]
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Record one request from `key` and return whether it is allowed.
    pub fn allow(&self, key: &str) -> bool {
        self.allow_at(key, Instant::now())
    }

    fn allow_at(&self, key: &str, now: Instant) -> bool {
        if self.limit == 0 {
            return true;
        }
        let mut entries = self.entries.lock();
        if entries.len() > 1024 {
            entries.retain(|_, (start, _)| now.saturating_duration_since(*start) < self.window);
        }
        let entry = entries.entry(key.to_string()).or_insert_with(|| (now, 0));
        if now.saturating_duration_since(entry.0) >= self.window {
            *entry = (now, 0);
        }
        let allowed = entry.1 < self.limit;
        if allowed {
            entry.1 += 1;
        }
        drop(entries);
        allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_requests_within_the_window() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        let now = Instant::now();
        assert!(limiter.allow_at("alice", now));
        assert!(limiter.allow_at("alice", now));
        assert!(!limiter.allow_at("alice", now));
        assert!(
            limiter.allow_at("bob", now),
            "clients have separate buckets"
        );
    }

    #[test]
    fn resets_after_the_window() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        let now = Instant::now();
        assert!(limiter.allow_at("alice", now));
        assert!(!limiter.allow_at("alice", now + Duration::from_secs(30)));
        assert!(limiter.allow_at("alice", now + Duration::from_secs(60)));
    }

    #[test]
    fn zero_disables_the_limiter() {
        let limiter = RateLimiter::new(0, Duration::from_secs(60));
        let now = Instant::now();
        for _ in 0..100 {
            assert!(limiter.allow_at("alice", now));
        }
    }
}
