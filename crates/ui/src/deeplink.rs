//! `jabu://` deep links.
//!
//! The public Host renders a `jabu://` link on every page. The app accepts the
//! same links from `jabu --open` and uses them to focus a contact, community,
//! Space, post or story.

use percent_encoding::percent_decode_str;

/// One parsed `jabu://` link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLink {
    Blog {
        jid: String,
    },
    Post {
        jid: String,
        item: String,
    },
    Contact {
        jid: String,
    },
    Community {
        service: String,
        node: String,
    },
    CommunityPost {
        service: String,
        node: String,
        item: String,
    },
    Space {
        service: String,
        node: String,
    },
    Story {
        jid: String,
        item: String,
    },
}

impl DeepLink {
    /// Parse a link such as `jabu://post/alice%40example.org/p1`.
    ///
    /// Both `jabu://` and `jabu:` prefixes are accepted. Path segments are
    /// percent-decoded. Invalid JIDs or missing parts return `None`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let rest = value.trim();
        let rest = rest
            .strip_prefix("jabu://")
            .or_else(|| rest.strip_prefix("jabu:"))?;
        let rest = rest.trim_start_matches('/');
        if rest.is_empty() {
            return None;
        }
        let segments = rest
            .split('/')
            .map(|segment| {
                percent_decode_str(segment)
                    .decode_utf8()
                    .ok()
                    .map(std::borrow::Cow::into_owned)
            })
            .collect::<Option<Vec<_>>>()?;
        match segments.as_slice() {
            [kind, jid] if kind == "blog" => Some(Self::Blog {
                jid: parse_jid(jid)?,
            }),
            [kind, jid] if kind == "contact" => Some(Self::Contact {
                jid: parse_jid(jid)?,
            }),
            [kind, jid, item] if kind == "post" => Some(Self::Post {
                jid: parse_jid(jid)?,
                item: required(item)?,
            }),
            [kind, service, node] if kind == "community" => Some(Self::Community {
                service: parse_jid(service)?,
                node: required(node)?,
            }),
            [kind, service, node, item] if kind == "community" => Some(Self::CommunityPost {
                service: parse_jid(service)?,
                node: required(node)?,
                item: required(item)?,
            }),
            [kind, service, node] if kind == "space" => Some(Self::Space {
                service: parse_jid(service)?,
                node: required(node)?,
            }),
            [kind, jid, item] if kind == "story" => Some(Self::Story {
                jid: parse_jid(jid)?,
                item: required(item)?,
            }),
            _ => None,
        }
    }

    /// Moderation target understood by the Host report API.
    #[must_use]
    pub fn target(&self) -> String {
        match self {
            Self::Blog { jid } | Self::Contact { jid } => format!("contact/{jid}"),
            Self::Post { jid, item } => format!("post/{jid}/{item}"),
            Self::Community { service, node } => format!("community/{service}/{node}"),
            Self::CommunityPost {
                service,
                node,
                item,
            } => format!("community/{service}/{node}/{item}"),
            Self::Space { service, node } => format!("space/{service}/{node}"),
            Self::Story { jid, item } => format!("story/{jid}/{item}"),
        }
    }

    /// Short human-readable description used by the UI.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Blog { jid } => format!("Blog of {jid}"),
            Self::Post { jid, .. } => format!("Post by {jid}"),
            Self::Contact { jid } => format!("Contact {jid}"),
            Self::Community { service, node } | Self::Space { service, node } => {
                format!("{service} / {node}")
            }
            Self::CommunityPost { service, node, .. } => format!("{service} / {node}"),
            Self::Story { jid, .. } => format!("Story by {jid}"),
        }
    }
}

fn parse_jid(value: &str) -> Option<String> {
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_whitespace) {
        return None;
    }
    let parsed = value.parse::<xmpp_core::Jid>().ok()?;
    parsed
        .to_bare()
        .to_string()
        .eq_ignore_ascii_case(value)
        .then(|| value.to_string())
}

fn required(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_host_link_shape() {
        assert_eq!(
            DeepLink::parse("jabu://blog/alice%40example%2Eorg"),
            Some(DeepLink::Blog {
                jid: "alice@example.org".to_string()
            })
        );
        assert_eq!(
            DeepLink::parse("jabu://post/alice@example.org/p1"),
            Some(DeepLink::Post {
                jid: "alice@example.org".to_string(),
                item: "p1".to_string()
            })
        );
        assert_eq!(
            DeepLink::parse("jabu://contact/alice@example.org"),
            Some(DeepLink::Contact {
                jid: "alice@example.org".to_string()
            })
        );
        assert_eq!(
            DeepLink::parse("jabu://community/pubsub.example.org/news"),
            Some(DeepLink::Community {
                service: "pubsub.example.org".to_string(),
                node: "news".to_string()
            })
        );
        assert_eq!(
            DeepLink::parse("jabu://community/pubsub.example.org/news/a1"),
            Some(DeepLink::CommunityPost {
                service: "pubsub.example.org".to_string(),
                node: "news".to_string(),
                item: "a1".to_string()
            })
        );
        assert_eq!(
            DeepLink::parse("jabu://space/pubsub.example.org/dev"),
            Some(DeepLink::Space {
                service: "pubsub.example.org".to_string(),
                node: "dev".to_string()
            })
        );
        assert_eq!(
            DeepLink::parse("jabu://story/alice@example.org/s1"),
            Some(DeepLink::Story {
                jid: "alice@example.org".to_string(),
                item: "s1".to_string()
            })
        );
    }

    #[test]
    fn accepts_the_short_prefix_and_extra_slashes() {
        assert_eq!(
            DeepLink::parse("jabu:/contact/alice@example.org"),
            Some(DeepLink::Contact {
                jid: "alice@example.org".to_string()
            })
        );
        assert_eq!(
            DeepLink::parse("  jabu:///contact/alice@example.org  "),
            Some(DeepLink::Contact {
                jid: "alice@example.org".to_string()
            })
        );
    }

    #[test]
    fn rejects_unknown_or_incomplete_links() {
        assert_eq!(DeepLink::parse("https://example.org/post/a/b"), None);
        assert_eq!(DeepLink::parse("jabu://"), None);
        assert_eq!(DeepLink::parse("jabu://post/only-one-part"), None);
        assert_eq!(DeepLink::parse("jabu://post//x"), None);
        assert_eq!(DeepLink::parse("jabu://post/not a jid/x"), None);
        assert_eq!(DeepLink::parse("jabu://space/service.example.org/"), None);
        assert_eq!(DeepLink::parse("jabu://unknown/x"), None);
    }

    #[test]
    fn reports_use_the_host_target_formats() {
        let cases = [
            ("jabu://blog/alice@example.org", "contact/alice@example.org"),
            (
                "jabu://post/alice@example.org/p1",
                "post/alice@example.org/p1",
            ),
            (
                "jabu://contact/alice@example.org",
                "contact/alice@example.org",
            ),
            (
                "jabu://community/pubsub.example.org/news",
                "community/pubsub.example.org/news",
            ),
            (
                "jabu://community/pubsub.example.org/news/a1",
                "community/pubsub.example.org/news/a1",
            ),
            (
                "jabu://space/pubsub.example.org/dev",
                "space/pubsub.example.org/dev",
            ),
            (
                "jabu://story/alice@example.org/s1",
                "story/alice@example.org/s1",
            ),
        ];
        for (link, target) in cases {
            assert_eq!(DeepLink::parse(link).unwrap().target(), target);
        }
    }
}
