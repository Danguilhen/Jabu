//! XEP-0115 entity capabilities.

use base64::Engine;
use minidom::Element;
use sha1::{Digest, Sha1};

pub const NODE: &str = "https://github.com/Danguilhen/Jabu/client";

/// Features this client advertises in its capability hash.
pub const FEATURES: &[&str] = &[
    "http://jabber.org/protocol/caps",
    "http://jabber.org/protocol/chatstates",
    "http://jabber.org/protocol/disco#info",
    "http://jabber.org/protocol/muc",
    "urn:xmpp:carbons:2",
    "urn:xmpp:chat-markers:0",
    "urn:xmpp:csi:0",
    "urn:xmpp:hints",
    "urn:xmpp:http:upload:0",
    "urn:xmpp:mam:2",
    "urn:xmpp:message-correct:0",
    "urn:xmpp:message-retract:0",
    "urn:xmpp:omemo:2",
    "urn:xmpp:receipts",
    "urn:xmpp:reply:0",
    "urn:xmpp:sm:3",
];

/// The `ver` attribute: SHA-1 over identity + node + sorted features, base64.
#[must_use]
pub fn verification() -> String {
    let mut features: Vec<&str> = FEATURES.to_vec();
    features.sort_unstable();
    let mut input = String::from("client/desktop//");
    input.push_str(NODE);
    input.push('#');
    for feature in features {
        input.push_str(feature);
        input.push('<');
    }
    let digest = Sha1::digest(input.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// Build the `<c/>` element carried by presence.
#[must_use]
pub fn caps_element() -> Element {
    Element::builder("c", "http://jabber.org/protocol/caps")
        .attr("hash", "sha-1")
        .attr("node", NODE)
        .attr("ver", verification())
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_is_stable() {
        assert_eq!(verification(), verification());
        assert!(!verification().is_empty());
    }

    #[test]
    fn element_advertises_the_node() {
        let caps = caps_element();
        assert_eq!(caps.name(), "c");
        assert_eq!(caps.attr("node"), Some(NODE));
        assert_eq!(caps.attr("hash"), Some("sha-1"));
        assert_eq!(caps.attr("ver"), Some(verification().as_str()));
    }
}
