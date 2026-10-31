//! XEP-0115 entity capabilities.

use base64::Engine;
use minidom::Element;
use sha1::{Digest, Sha1};
use sha2::Sha256;

pub const NODE: &str = "https://github.com/Danguilhen/Jabu/client";

/// Features this client advertises in its capability hash.
pub const FEATURES: &[&str] = &[
    "http://jabber.org/protocol/caps",
    "http://jabber.org/protocol/chatstates",
    "urn:xmpp:caps",
    "http://jabber.org/protocol/disco#info",
    "http://jabber.org/protocol/muc",
    "urn:xmpp:activity:0+notify",
    "urn:xmpp:avatar:metadata+notify",
    "urn:xmpp:bob",
    "urn:xmpp:carbons:2",
    "urn:xmpp:chat-markers:0",
    "urn:xmpp:csi:0",
    "urn:xmpp:hints",
    "urn:xmpp:http:upload:0",
    "urn:xmpp:jingle:1",
    "urn:xmpp:jingle:apps:dtls:0",
    "urn:xmpp:jingle:apps:rtp:1",
    "urn:xmpp:jingle:apps:rtp:audio",
    "urn:xmpp:jingle:apps:rtp:rtcp-fb:0",
    "urn:xmpp:jingle:apps:rtp:rtp-hdrext:0",
    "urn:xmpp:jingle:apps:rtp:video",
    "urn:xmpp:jingle:apps:grouping:0",
    "urn:xmpp:jingle:transports:ice-udp:1",
    "urn:xmpp:jingle-message:0",
    "urn:xmpp:mam:2",
    "urn:xmpp:message-correct:0",
    "urn:xmpp:message-retract:0",
    "urn:xmpp:microblog:0+notify",
    "urn:xmpp:pubsub-social-feed:1",
    "urn:xmpp:pubsub-social-feed:stories:0",
    "urn:xmpp:pubsub-social-feed:stories:0+notify",
    "urn:xmpp:mood:0+notify",
    "urn:xmpp:omemo:2",
    "urn:xmpp:omemo:2:devices+notify",
    "urn:xmpp:receipts",
    "urn:xmpp:reply:0",
    "urn:xmpp:sm:3",
    "urn:xmpp:tune:0+notify",
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

/// Build the XEP-0115 `<c/>` element carried by presence.
#[must_use]
pub fn caps_element() -> Element {
    Element::builder("c", "http://jabber.org/protocol/caps")
        .attr("hash", "sha-1")
        .attr("node", NODE)
        .attr("ver", verification())
        .build()
}

/// The XEP-0390 hash input for our current identity and feature set.
fn caps2_input() -> Vec<u8> {
    let mut features: Vec<&str> = FEATURES.to_vec();
    features.sort_unstable();

    let mut input = Vec::new();
    for feature in features {
        input.extend_from_slice(feature.as_bytes());
        input.push(0x1f);
    }
    input.push(0x1c);

    for field in ["client", "desktop", "", ""] {
        input.extend_from_slice(field.as_bytes());
        input.push(0x1f);
    }
    input.push(0x1e);
    input.push(0x1c);
    // No XEP-0128 extended info forms.
    input.push(0x1c);
    input
}

/// The XEP-0390 SHA-256 capability hash, base64 encoded.
#[must_use]
pub fn verification_sha256() -> String {
    let digest = Sha256::digest(caps2_input());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// Build the XEP-0390 `<c/>` element carried by presence.
#[must_use]
pub fn caps2_element() -> Element {
    Element::builder("c", "urn:xmpp:caps")
        .append(
            Element::builder("hash", "urn:xmpp:hashes:2")
                .attr("algo", "sha-256")
                .append(verification_sha256()),
        )
        .build()
}

/// Both capability elements advertised in presence.
#[must_use]
pub fn caps_elements() -> Vec<Element> {
    vec![caps_element(), caps2_element()]
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
    fn caps2_advertises_a_sha256_hash() {
        let caps = caps2_element();
        assert_eq!(caps.name(), "c");
        assert_eq!(caps.ns(), "urn:xmpp:caps");
        let hash = caps.get_child("hash", "urn:xmpp:hashes:2").expect("hash");
        assert_eq!(hash.attr("algo"), Some("sha-256"));
        assert_eq!(hash.text(), verification_sha256());
        assert_eq!(verification_sha256().len(), 44);
        assert_ne!(verification_sha256(), verification());
    }

    #[test]
    fn advertisements_include_both_protocol_versions() {
        let elements = caps_elements();
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].ns(), "http://jabber.org/protocol/caps");
        assert_eq!(elements[1].ns(), "urn:xmpp:caps");
        assert!(FEATURES.contains(&"urn:xmpp:caps"));
    }

    #[test]
    fn element_advertises_the_node() {
        let caps = caps_element();
        assert_eq!(caps.name(), "c");
        assert_eq!(caps.attr("node"), Some(NODE));
        assert_eq!(caps.attr("hash"), Some("sha-1"));
        assert_eq!(caps.attr("ver"), Some(verification().as_str()));
    }

    #[test]
    fn advertises_pep_notify_features() {
        assert!(FEATURES.contains(&"urn:xmpp:microblog:0+notify"));
        assert!(FEATURES.contains(&"urn:xmpp:avatar:metadata+notify"));
        assert!(FEATURES.contains(&"urn:xmpp:omemo:2:devices+notify"));
    }
}
