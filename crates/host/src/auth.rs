//! Admin authentication primitives.
//!
//! Admin access is granted by a token. Tokens are stored as SHA-256 hashes,
//! exchanged for a short-lived server-side session, and carried either in an
//! `HttpOnly` cookie (browser) or as a bearer token (JSON API).

use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Lifetime of an admin session created by the login form.
pub const SESSION_TTL_HOURS: i64 = 24;

/// Access level of an admin token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Full access: settings, tokens, backups and moderation.
    Admin,
    /// Report queue, moderation actions and audit log.
    Moderator,
}

impl Role {
    /// Stable slug used in the database and forms.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Moderator => "moderator",
        }
    }

    /// Parse a role slug.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "admin" => Some(Self::Admin),
            "moderator" => Some(Self::Moderator),
            _ => None,
        }
    }

    /// Whether this role satisfies the `required` role. Admins may do
    /// everything a moderator can.
    #[must_use]
    pub const fn allows(self, required: Self) -> bool {
        matches!(self, Self::Admin) || matches!(required, Self::Moderator)
    }
}

/// One admin token as shown in the token list. The token secret itself is
/// never stored and cannot be recovered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TokenSummary {
    pub id: String,
    pub label: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
}

/// Identity established by a bearer token or an admin session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authenticated {
    pub label: String,
    pub role: Role,
}

/// SHA-256 hash of a token or session id, hex encoded.
#[must_use]
pub fn hash_secret(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// A fresh, URL-safe secret with 122 bits of entropy.
#[must_use]
pub fn new_secret(prefix: &str) -> String {
    format!("{prefix}{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_parse_and_allow() {
        assert_eq!(Role::parse("admin"), Some(Role::Admin));
        assert_eq!(Role::parse("moderator"), Some(Role::Moderator));
        assert_eq!(Role::parse("owner"), None);
        assert!(Role::Admin.allows(Role::Moderator));
        assert!(Role::Moderator.allows(Role::Moderator));
        assert!(!Role::Moderator.allows(Role::Admin));
    }

    #[test]
    fn hashes_are_stable_and_differ_by_input() {
        assert_eq!(hash_secret("token"), hash_secret("token"));
        assert_ne!(hash_secret("token"), hash_secret("token2"));
        assert_eq!(hash_secret("token").len(), 64);
    }

    #[test]
    fn secrets_are_prefixed_and_unique() {
        let first = new_secret("jabu_");
        let second = new_secret("jabu_");
        assert!(first.starts_with("jabu_"));
        assert_ne!(first, second);
    }
}
