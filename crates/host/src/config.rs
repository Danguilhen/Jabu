//! Host configuration.

use serde::{Deserialize, Serialize};

/// Public Host settings, read from the `[host]` section of the app config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HostConfig {
    /// Address to bind, e.g. `127.0.0.1:8088`. Put a TLS reverse proxy in front.
    pub bind: String,
    /// Canonical base URL used for links and `OpenGraph` cards.
    pub public_url: Option<String>,
    /// Instance name shown on the pages.
    pub site_name: String,
    /// Instance description used for `<meta name="description">`.
    pub description: String,
    /// How long a fetched public node is cached, in seconds.
    pub cache_ttl_secs: u64,
    /// Number of posts fetched for a blog page.
    pub blog_page_size: u32,
    /// Public blog JIDs listed in `sitemap.xml`.
    pub featured_blogs: Vec<String>,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8088".to_string(),
            public_url: None,
            site_name: "Jabu".to_string(),
            description: "Public pages served by a Jabu instance.".to_string(),
            cache_ttl_secs: 300,
            blog_page_size: 20,
            featured_blogs: Vec::new(),
        }
    }
}

impl HostConfig {
    /// Absolute URL for a path, falling back to a relative link without a
    /// configured public URL.
    #[must_use]
    pub fn absolute(&self, path: &str) -> String {
        self.public_url.as_deref().map_or_else(
            || path.to_string(),
            |base| format!("{}{}", base.trim_end_matches('/'), path),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_absolute_urls_without_doubling_slashes() {
        let config = HostConfig {
            public_url: Some("https://jabu.example/".to_string()),
            ..HostConfig::default()
        };
        assert_eq!(
            config.absolute("/blog/alice@example.org"),
            "https://jabu.example/blog/alice@example.org"
        );
    }

    #[test]
    fn keeps_paths_relative_without_a_public_url() {
        let config = HostConfig::default();
        assert_eq!(config.absolute("/about"), "/about");
    }
}
