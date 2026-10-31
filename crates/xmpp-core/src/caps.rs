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

const NS_DISCO_INFO: &str = "http://jabber.org/protocol/disco#info";
const NS_DATA_FORMS: &str = "jabber:x:data";

/// Build the XEP-0390 hash input from a `disco#info` query.
///
/// Returns `None` for elements that are not valid capability input (unknown
/// children, malformed forms or a form containing `reported`/`item`).
fn caps2_input_from_query(query: &Element) -> Option<Vec<u8>> {
    let mut features: Vec<Vec<u8>> = Vec::new();
    let mut identities: Vec<Vec<u8>> = Vec::new();
    let mut forms: Vec<Vec<u8>> = Vec::new();

    for child in query.children() {
        if child.name() == "feature" && child.ns() == NS_DISCO_INFO {
            let mut value = child.attr("var").unwrap_or_default().as_bytes().to_vec();
            value.push(0x1f);
            features.push(value);
        } else if child.name() == "identity" && child.ns() == NS_DISCO_INFO {
            let mut value = Vec::new();
            for field in [
                child.attr("category").unwrap_or_default(),
                child.attr("type").unwrap_or_default(),
                child.attr("xml:lang").unwrap_or_default(),
                child.attr("name").unwrap_or_default(),
            ] {
                value.extend_from_slice(field.as_bytes());
                value.push(0x1f);
            }
            value.push(0x1e);
            identities.push(value);
        } else if child.name() == "x" && child.ns() == NS_DATA_FORMS {
            if child.get_child("reported", NS_DATA_FORMS).is_some()
                || child.get_child("item", NS_DATA_FORMS).is_some()
            {
                return None;
            }
            let mut fields: Vec<Vec<u8>> = Vec::new();
            for field in child.children().filter(|field| field.name() == "field") {
                let mut value = field.attr("var").unwrap_or_default().as_bytes().to_vec();
                value.push(0x1f);
                let mut values: Vec<Vec<u8>> = field
                    .children()
                    .filter(|child| child.name() == "value")
                    .map(|value| value.text().into_bytes())
                    .collect();
                values.sort();
                for item in values {
                    value.extend_from_slice(&item);
                    value.push(0x1f);
                }
                value.push(0x1e);
                fields.push(value);
            }
            fields.sort();
            let mut form = Vec::new();
            for field in fields {
                form.extend_from_slice(&field);
            }
            form.push(0x1d);
            forms.push(form);
        } else {
            return None;
        }
    }

    features.sort();
    identities.sort();
    forms.sort();
    let mut input = Vec::new();
    for feature in features {
        input.extend_from_slice(&feature);
    }
    input.push(0x1c);
    for identity in identities {
        input.extend_from_slice(&identity);
    }
    input.push(0x1c);
    for form in forms {
        input.extend_from_slice(&form);
    }
    input.push(0x1c);
    Some(input)
}

/// The XEP-0390 SHA-256 capability hash for a `disco#info` query.
#[must_use]
pub fn caps2_hash_for_query(query: &Element) -> Option<String> {
    let input = caps2_input_from_query(query)?;
    let digest = Sha256::digest(input);
    Some(base64::engine::general_purpose::STANDARD.encode(digest))
}

/// Build a `disco#info` query describing this client.
#[must_use]
pub fn own_disco_info() -> Element {
    let mut query = Element::builder("query", NS_DISCO_INFO).append(
        Element::builder("identity", NS_DISCO_INFO)
            .attr("category", "client")
            .attr("type", "desktop")
            .attr("name", "Jabu"),
    );
    for feature in FEATURES {
        query = query.append(Element::builder("feature", NS_DISCO_INFO).attr("var", *feature));
    }
    query.build()
}

/// The XEP-0390 SHA-256 capability hash, base64 encoded.
#[must_use]
pub fn verification_sha256() -> String {
    caps2_hash_for_query(&own_disco_info()).unwrap_or_default()
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
    use std::str::FromStr;

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
    fn caps2_hash_matches_own_disco_info() {
        assert_eq!(
            caps2_hash_for_query(&own_disco_info()).as_deref(),
            Some(verification_sha256().as_str())
        );
    }

    #[test]
    fn caps2_hash_includes_extended_info_forms() {
        let query = Element::from_str(
            r#"<query xmlns="http://jabber.org/protocol/disco#info"><identity category="client" type="pc"/><feature var="urn:xmpp:test"/><x xmlns="jabber:x:data" type="result"><field var="FORM_TYPE"><value>urn:xmpp:test:info</value></field></x></query>"#,
        )
        .unwrap();
        let with_form = caps2_hash_for_query(&query).expect("hash");
        let query_without = Element::from_str(
            r#"<query xmlns="http://jabber.org/protocol/disco#info"><identity category="client" type="pc"/><feature var="urn:xmpp:test"/></query>"#,
        )
        .unwrap();
        assert_ne!(
            with_form,
            caps2_hash_for_query(&query_without).expect("hash")
        );
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
