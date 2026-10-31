//! XEP-0330: Pubsub Subscription.
//!
//! The account publishes the nodes it follows to the PEP node
//! `urn:xmpp:pubsub:subscription`, one item per subscription. The item id is a
//! SHA-1 of `server < node < bare JID`, which matches Movim and lets either
//! client retract the same item without extra state.
//!
//! The node uses the `presence` access model, so contacts (and anyone with a
//! presence subscription) can discover what an account follows.

use jid::Jid;
use minidom::{Element, NSChoice};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

/// PEP node that holds the public subscription list.
pub const NS_SUBSCRIPTION: &str = "urn:xmpp:pubsub:subscription";

/// Namespace of one `<subscription/>` item.
pub const NS_SUBSCRIPTION_ITEM: &str = "urn:xmpp:pubsub:subscription:0";

/// Movim's per-room notification extension.
pub const NS_NOTIFICATION_SETTINGS: &str = "urn:xmpp:notification-settings:0";

/// Movim's bookmark pinning extension.
pub const NS_BOOKMARKS_PINNING: &str = "urn:xmpp:bookmarks-pinning:0";

const NS_PUBSUB: &str = "http://jabber.org/protocol/pubsub";

/// One node the account follows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionEntry {
    pub server: String,
    pub node: String,
    #[serde(default)]
    pub title: Option<String>,
    /// Canonical Movim notification level: `always`, `mentions` or `never`.
    #[serde(default)]
    pub notify: Option<String>,
    #[serde(default)]
    pub pinned: bool,
}

impl SubscriptionEntry {
    #[must_use]
    pub fn item_id(&self, own: &Jid) -> String {
        item_id(&self.server, &self.node, own)
    }
}

/// Stable item id used by Movim for a subscription.
#[must_use]
pub fn item_id(server: &str, node: &str, own: &Jid) -> String {
    let digest = Sha1::digest(format!("{server}<{node}<{}", own.to_bare()).as_bytes());
    format!("{digest:x}")
}

fn notify_element(value: &str) -> &'static str {
    match value {
        "never" => "never",
        "mentions" | "on-mention" => "on-mention",
        _ => "always",
    }
}

fn canonical_notify(value: &str) -> String {
    match value {
        "never" => "never".to_string(),
        "on-mention" | "mentions" => "mentions".to_string(),
        _ => "always".to_string(),
    }
}

fn extensions_element(entry: &SubscriptionEntry) -> Option<Element> {
    if entry.notify.is_none() && !entry.pinned {
        return None;
    }
    let mut extensions = Element::builder("extensions", NS_SUBSCRIPTION_ITEM);
    if let Some(notify) = entry.notify.as_deref() {
        let name = notify_element(notify);
        extensions = extensions.append(
            Element::builder("notify", NS_NOTIFICATION_SETTINGS)
                .append(Element::builder(name, NS_NOTIFICATION_SETTINGS)),
        );
    }
    if entry.pinned {
        extensions = extensions.append(Element::builder("pinned", NS_BOOKMARKS_PINNING));
    }
    Some(extensions.build())
}

fn subscription_element(entry: &SubscriptionEntry, own: &Jid) -> Element {
    let mut subscription = Element::builder("subscription", NS_SUBSCRIPTION_ITEM)
        .attr("server", entry.server.clone())
        .attr("node", entry.node.clone());
    if let Some(title) = entry.title.as_deref().filter(|title| !title.is_empty()) {
        subscription = subscription
            .append(Element::builder("title", NS_SUBSCRIPTION_ITEM).append(title.to_string()));
    }
    if let Some(extensions) = extensions_element(entry) {
        subscription = subscription.append(extensions);
    }
    Element::builder("item", NS_PUBSUB)
        .attr("id", item_id(&entry.server, &entry.node, own))
        .append(subscription.build())
        .build()
}

fn publish_options() -> Element {
    let mut form = Element::builder("x", crate::data_form::NS_DATA_FORMS).attr("type", "submit");
    for (var, value) in [
        (
            "FORM_TYPE",
            "http://jabber.org/protocol/pubsub#publish-options",
        ),
        ("pubsub#persist_items", "true"),
        ("pubsub#access_model", "presence"),
        ("pubsub#send_last_published_item", "never"),
        ("pubsub#max_items", "max"),
        ("pubsub#notify_retract", "true"),
    ] {
        let mut field = Element::builder("field", crate::data_form::NS_DATA_FORMS).attr("var", var);
        if var == "FORM_TYPE" {
            field = field.attr("type", "hidden");
        }
        form = form.append(
            field.append(Element::builder("value", crate::data_form::NS_DATA_FORMS).append(value)),
        );
    }
    Element::builder("publish-options", NS_PUBSUB)
        .append(form.build())
        .build()
}

/// Build the IQ that publishes or updates one public subscription.
#[must_use]
pub fn publish_iq(own: &Jid, entry: &SubscriptionEntry) -> Element {
    Element::builder("iq", xmpp_parsers::ns::JABBER_CLIENT)
        .attr("type", "set")
        .attr("to", own.to_bare().to_string())
        .attr(
            "id",
            format!("subscription-publish-{}", uuid::Uuid::new_v4()),
        )
        .append(
            Element::builder("pubsub", NS_PUBSUB)
                .append(
                    Element::builder("publish", NS_PUBSUB)
                        .attr("node", NS_SUBSCRIPTION)
                        .append(subscription_element(entry, own)),
                )
                .append(publish_options()),
        )
        .build()
}

/// Build the IQ that retracts one public subscription.
#[must_use]
pub fn retract_iq(own: &Jid, server: &str, node: &str) -> Element {
    Element::builder("iq", xmpp_parsers::ns::JABBER_CLIENT)
        .attr("type", "set")
        .attr("to", own.to_bare().to_string())
        .attr(
            "id",
            format!("subscription-retract-{}", uuid::Uuid::new_v4()),
        )
        .append(
            Element::builder("pubsub", NS_PUBSUB).append(
                Element::builder("retract", NS_PUBSUB)
                    .attr("node", NS_SUBSCRIPTION)
                    .append(
                        Element::builder("item", NS_PUBSUB).attr("id", item_id(server, node, own)),
                    ),
            ),
        )
        .build()
}

/// Parse the `<items/>` result of the public subscription node.
#[must_use]
pub fn parse_items(items: &Element) -> Vec<SubscriptionEntry> {
    items
        .children()
        .filter(|child| child.name() == "item")
        .filter_map(|item| {
            let subscription = item.get_child("subscription", NSChoice::Any)?;
            let server = subscription.attr("server")?.to_string();
            let node = subscription.attr("node")?.to_string();
            let title = subscription
                .get_child("title", NSChoice::Any)
                .map(Element::text)
                .filter(|title| !title.is_empty());
            let extensions = subscription.get_child("extensions", NSChoice::Any);
            let notify = extensions
                .and_then(|extensions| extensions.get_child("notify", NS_NOTIFICATION_SETTINGS))
                .and_then(|notify| notify.children().next())
                .map(|element| canonical_notify(element.name()));
            let pinned = extensions.is_some_and(|extensions| {
                extensions
                    .get_child("pinned", NS_BOOKMARKS_PINNING)
                    .is_some()
            });
            Some(SubscriptionEntry {
                server,
                node,
                title,
                notify,
                pinned,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn own() -> Jid {
        "romeo@montague.lit".parse().expect("own jid")
    }

    #[test]
    fn item_id_matches_the_xep_example() {
        assert_eq!(
            item_id("pubsub.shakespeare.lit", "party", &own()),
            "0bc0e76cb803b3b107aa369169d8c0d45086f844"
        );
    }

    #[test]
    fn publishes_an_item_with_options_and_extensions() {
        let entry = SubscriptionEntry {
            server: "pubsub.shakespeare.lit".to_string(),
            node: "party".to_string(),
            title: Some("Party at the Capulets".to_string()),
            notify: Some("mentions".to_string()),
            pinned: true,
        };
        let iq = publish_iq(&own(), &entry);
        let pubsub = iq.get_child("pubsub", NS_PUBSUB).expect("pubsub");
        let publish = pubsub.get_child("publish", NS_PUBSUB).expect("publish");
        assert_eq!(publish.attr("node"), Some(NS_SUBSCRIPTION));
        let item = publish.get_child("item", NS_PUBSUB).expect("item");
        assert_eq!(
            item.attr("id"),
            Some("0bc0e76cb803b3b107aa369169d8c0d45086f844")
        );
        let subscription = item
            .get_child("subscription", NS_SUBSCRIPTION_ITEM)
            .expect("subscription");
        assert_eq!(subscription.attr("server"), Some("pubsub.shakespeare.lit"));
        assert_eq!(subscription.attr("node"), Some("party"));
        assert_eq!(
            subscription
                .get_child("title", NSChoice::Any)
                .map(Element::text)
                .as_deref(),
            Some("Party at the Capulets")
        );
        let notify = subscription
            .get_child("extensions", NSChoice::Any)
            .and_then(|extensions| extensions.get_child("notify", NS_NOTIFICATION_SETTINGS))
            .expect("notify");
        assert!(notify
            .get_child("on-mention", NS_NOTIFICATION_SETTINGS)
            .is_some());
        assert!(subscription
            .get_child("extensions", NSChoice::Any)
            .and_then(|extensions| extensions.get_child("pinned", NS_BOOKMARKS_PINNING))
            .is_some());

        let options = pubsub
            .get_child("publish-options", NS_PUBSUB)
            .and_then(|options| options.get_child("x", crate::data_form::NS_DATA_FORMS))
            .expect("form");
        let access = options
            .children()
            .find(|field| field.attr("var") == Some("pubsub#access_model"))
            .and_then(|field| field.get_child("value", crate::data_form::NS_DATA_FORMS))
            .map(Element::text);
        assert_eq!(access.as_deref(), Some("presence"));
    }

    #[test]
    fn parses_items_with_extensions() {
        let items = Element::from_str(
            r#"<items xmlns="http://jabber.org/protocol/pubsub" node="urn:xmpp:pubsub:subscription">
                 <item id="0bc0e76cb803b3b107aa369169d8c0d45086f844">
                   <subscription xmlns="urn:xmpp:pubsub:subscription:0" server="pubsub.shakespeare.lit" node="party">
                     <title>Party at the Capulets</title>
                     <extensions>
                       <notify xmlns="urn:xmpp:notification-settings:0"><never/></notify>
                     </extensions>
                   </subscription>
                 </item>
               </items>"#,
        )
        .expect("items");
        let entries = parse_items(&items);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].server, "pubsub.shakespeare.lit");
        assert_eq!(entries[0].node, "party");
        assert_eq!(entries[0].title.as_deref(), Some("Party at the Capulets"));
        assert_eq!(entries[0].notify.as_deref(), Some("never"));
        assert!(!entries[0].pinned);
    }
}
