//! Editable instance settings.
//!
//! Presentation and retention values can be changed from `/admin/settings`
//! and are stored in the moderation SQLite database. Networking and cache
//! values stay in the configuration file because changing them at runtime
//! would require rebuilding the listener or the content cache.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::HostConfig;

/// Default link/accent colour.
pub const DEFAULT_ACCENT: &str = "#7dd3fc";

/// Limits enforced by [`RuntimeSettings::validate`].
const MAX_SITE_NAME: usize = 80;
const MAX_DESCRIPTION: usize = 500;
const MAX_FOOTER_NOTE: usize = 500;
const MAX_FEATURED_BLOGS: usize = 50;
const MAX_RETENTION_DAYS: u32 = 3650;

/// The subset of [`HostConfig`] that can be edited at runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSettings {
    pub site_name: String,
    pub description: String,
    pub featured_blogs: Vec<String>,
    /// Days to keep dismissed reports and audit entries. `0` keeps them.
    pub retention_days: u32,
    /// Link and accent colour, `#rrggbb`.
    pub accent_color: String,
    /// Optional note appended to the page footer.
    pub footer_note: String,
}

/// A rejected instance setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsError {
    SiteNameRequired,
    SiteNameTooLong(usize),
    DescriptionTooLong(usize),
    FooterNoteTooLong(usize),
    TooManyFeaturedBlogs(usize),
    InvalidJid(String),
    RetentionTooLong(usize),
    InvalidAccent,
}

impl SettingsError {
    /// Localized explanation for the administrator.
    #[must_use]
    pub fn message(&self, i18n: &jabu_i18n::Translator) -> String {
        let mut args = jabu_i18n::Args::new();
        match self {
            Self::SiteNameRequired => i18n.t("host-admin-site-required"),
            Self::SiteNameTooLong(count) => {
                args.set("count", *count as i64);
                i18n.t_args("host-admin-site-too-long", &args)
            }
            Self::DescriptionTooLong(count) => {
                args.set("count", *count as i64);
                i18n.t_args("host-admin-description-too-long", &args)
            }
            Self::FooterNoteTooLong(count) => {
                args.set("count", *count as i64);
                i18n.t_args("host-admin-footer-too-long", &args)
            }
            Self::TooManyFeaturedBlogs(count) => {
                args.set("count", *count as i64);
                i18n.t_args("host-admin-too-many-blogs", &args)
            }
            Self::InvalidJid(jid) => {
                args.set("jid", jid.clone());
                i18n.t_args("host-admin-invalid-jid", &args)
            }
            Self::RetentionTooLong(count) => {
                args.set("count", *count as i64);
                i18n.t_args("host-admin-retention-too-long", &args)
            }
            Self::InvalidAccent => i18n.t("host-admin-accent-invalid"),
        }
    }
}

impl RuntimeSettings {
    /// Values currently in effect for a configuration.
    #[must_use]
    pub fn from_config(config: &HostConfig) -> Self {
        Self {
            site_name: config.site_name.clone(),
            description: config.description.clone(),
            featured_blogs: config.featured_blogs.clone(),
            retention_days: config.retention_days,
            accent_color: if is_hex_color(&config.accent_color) {
                config.accent_color.clone()
            } else {
                DEFAULT_ACCENT.to_string()
            },
            footer_note: config.footer_note.clone(),
        }
    }

    /// Load settings, letting stored values override the configuration file.
    #[must_use]
    pub fn load(overrides: &HashMap<String, String>, config: &HostConfig) -> Self {
        let mut settings = Self::from_config(config);
        if let Some(value) = overrides.get("site_name").filter(|value| !value.is_empty()) {
            settings.site_name.clone_from(value);
        }
        if let Some(value) = overrides.get("description") {
            settings.description.clone_from(value);
        }
        if let Some(value) = overrides
            .get("featured_blogs")
            .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
        {
            settings.featured_blogs = value;
        }
        if let Some(value) = overrides
            .get("retention_days")
            .and_then(|value| value.parse().ok())
        {
            settings.retention_days = value;
        }
        if let Some(value) = overrides
            .get("accent_color")
            .filter(|value| is_hex_color(value))
        {
            settings.accent_color.clone_from(value);
        }
        if let Some(value) = overrides.get("footer_note") {
            settings.footer_note.clone_from(value);
        }
        settings
    }

    /// Apply these settings to a configuration, leaving immutable fields
    /// (bind address, public URL, cache, rate limit, database) untouched.
    pub fn apply(&self, config: &mut HostConfig) {
        config.site_name.clone_from(&self.site_name);
        config.description.clone_from(&self.description);
        config.featured_blogs.clone_from(&self.featured_blogs);
        config.retention_days = self.retention_days;
        config.accent_color.clone_from(&self.accent_color);
        config.footer_note.clone_from(&self.footer_note);
    }

    /// Check user-facing limits.
    ///
    /// # Errors
    ///
    /// Returns the first invalid field as a typed error, so callers can
    /// localize the message.
    pub fn validate(&self) -> Result<(), SettingsError> {
        let site_name = self.site_name.trim();
        if site_name.is_empty() {
            return Err(SettingsError::SiteNameRequired);
        }
        if site_name.chars().count() > MAX_SITE_NAME {
            return Err(SettingsError::SiteNameTooLong(MAX_SITE_NAME));
        }
        if self.description.chars().count() > MAX_DESCRIPTION {
            return Err(SettingsError::DescriptionTooLong(MAX_DESCRIPTION));
        }
        if self.footer_note.chars().count() > MAX_FOOTER_NOTE {
            return Err(SettingsError::FooterNoteTooLong(MAX_FOOTER_NOTE));
        }
        if self.featured_blogs.len() > MAX_FEATURED_BLOGS {
            return Err(SettingsError::TooManyFeaturedBlogs(MAX_FEATURED_BLOGS));
        }
        for blog in &self.featured_blogs {
            if blog.parse::<jid::Jid>().is_err() {
                return Err(SettingsError::InvalidJid(blog.clone()));
            }
        }
        if self.retention_days > MAX_RETENTION_DAYS {
            return Err(SettingsError::RetentionTooLong(MAX_RETENTION_DAYS as usize));
        }
        if !is_hex_color(&self.accent_color) {
            return Err(SettingsError::InvalidAccent);
        }
        Ok(())
    }

    /// Key/value rows persisted in the database.
    #[must_use]
    pub fn pairs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("site_name", self.site_name.clone()),
            ("description", self.description.clone()),
            (
                "featured_blogs",
                serde_json::to_string(&self.featured_blogs).unwrap_or_else(|_| "[]".to_string()),
            ),
            ("retention_days", self.retention_days.to_string()),
            ("accent_color", self.accent_color.clone()),
            ("footer_note", self.footer_note.clone()),
        ]
    }

    /// Parse a newline- or comma-separated list of featured blog JIDs.
    #[must_use]
    pub fn parse_blog_list(value: &str) -> Vec<String> {
        value
            .split(['\n', ',', ' ', '\t'])
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(str::to_string)
            .collect()
    }
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self::from_config(&HostConfig::default())
    }
}

/// Whether `value` is a `#rrggbb` colour.
#[must_use]
pub fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_values_override_the_configuration() {
        let config = HostConfig::default();
        let mut overrides = HashMap::new();
        overrides.insert("site_name".to_string(), "Example".to_string());
        overrides.insert("retention_days".to_string(), "7".to_string());
        overrides.insert(
            "featured_blogs".to_string(),
            "[\"alice@example.org\"]".to_string(),
        );
        let settings = RuntimeSettings::load(&overrides, &config);
        assert_eq!(settings.site_name, "Example");
        assert_eq!(settings.retention_days, 7);
        assert_eq!(settings.featured_blogs, vec!["alice@example.org"]);
        assert_eq!(settings.description, config.description);
    }

    #[test]
    fn apply_leaves_networking_fields_alone() {
        let mut config = HostConfig {
            bind: "0.0.0.0:9000".to_string(),
            ..HostConfig::default()
        };
        let settings = RuntimeSettings {
            site_name: "New".to_string(),
            description: "Desc".to_string(),
            featured_blogs: Vec::new(),
            retention_days: 5,
            accent_color: "#ffffff".to_string(),
            footer_note: "Hello".to_string(),
        };
        settings.apply(&mut config);
        assert_eq!(config.site_name, "New");
        assert_eq!(config.bind, "0.0.0.0:9000");
        assert_eq!(config.retention_days, 5);
        assert_eq!(config.accent_color, "#ffffff");
    }

    #[test]
    fn validation_rejects_bad_values() {
        let mut settings = RuntimeSettings::default();
        assert!(settings.validate().is_ok());
        settings.accent_color = "red".to_string();
        assert!(settings.validate().is_err());
        settings.accent_color = "#12345".to_string();
        assert!(settings.validate().is_err());
        settings.accent_color = "#123456".to_string();
        settings.site_name.clear();
        assert!(settings.validate().is_err());
        settings.site_name = "Fine".to_string();
        settings.featured_blogs = vec![String::new()];
        assert!(settings.validate().is_err());
    }

    #[test]
    fn validation_messages_are_localized() {
        let settings = RuntimeSettings {
            site_name: String::new(),
            ..RuntimeSettings::default()
        };
        let error = settings.validate().unwrap_err();
        assert_eq!(
            error.message(&jabu_i18n::Translator::for_tag("en")),
            "Site name is required."
        );
        assert_eq!(
            error.message(&jabu_i18n::Translator::for_tag("fr")),
            "Le nom du site est requis."
        );
    }

    #[test]
    fn blog_lists_accept_commas_and_newlines() {
        assert_eq!(
            RuntimeSettings::parse_blog_list("a@example.org, b@example.org\nc@example.org"),
            vec!["a@example.org", "b@example.org", "c@example.org"]
        );
    }
}
