//! XEP-0497: `PubSub` Extended Subscriptions.
//!
//! Adds subscription options so a subscriber can follow a node hierarchy
//! (through XEP-0496 `parent` relationships) and receive linked-node or
//! metadata notifications. The options travel in a
//! `http://jabber.org/protocol/pubsub#subscribe_options` data form inside the
//! `<subscribe/>` request.

use jid::Jid;
use minidom::Element;

/// XEP-0497 namespace and advertised feature.
pub const NS_PUBSUB_EXT_SUB: &str = "urn:xmpp:pubsub-ext-sub:0";

const NS_PUBSUB: &str = "http://jabber.org/protocol/pubsub";
const NS_SUBSCRIBE_OPTIONS: &str = "http://jabber.org/protocol/pubsub#subscribe_options";

/// What a subscription should notify about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionType {
    Items,
    LinkedItems,
    Metadata,
}

impl SubscriptionType {
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Items => "items",
            Self::LinkedItems => "linked items",
            Self::Metadata => "metadata",
        }
    }
}

/// XEP-0497 subscription options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedSubscription {
    pub types: Vec<SubscriptionType>,
    /// `0` is this node only, `1` direct children, `-1` unlimited.
    pub depth: i32,
}

impl Default for ExtendedSubscription {
    fn default() -> Self {
        Self {
            types: vec![SubscriptionType::Items],
            depth: 0,
        }
    }
}

fn options_form(options: &ExtendedSubscription) -> Element {
    let mut types = Element::builder("field", crate::data_form::NS_DATA_FORMS)
        .attr("var", format!("{{{NS_PUBSUB_EXT_SUB}}}type"))
        .attr("type", "list-multi");
    for kind in &options.types {
        types = types
            .append(Element::builder("value", crate::data_form::NS_DATA_FORMS).append(kind.slug()));
    }
    Element::builder("x", crate::data_form::NS_DATA_FORMS)
        .attr("type", "submit")
        .append(
            Element::builder("field", crate::data_form::NS_DATA_FORMS)
                .attr("var", "FORM_TYPE")
                .attr("type", "hidden")
                .append(
                    Element::builder("value", crate::data_form::NS_DATA_FORMS)
                        .append(NS_SUBSCRIBE_OPTIONS),
                ),
        )
        .append(types.build())
        .append(
            Element::builder("field", crate::data_form::NS_DATA_FORMS)
                .attr("var", format!("{{{NS_PUBSUB_EXT_SUB}}}depth"))
                .attr("type", "text-single")
                .append(
                    Element::builder("value", crate::data_form::NS_DATA_FORMS)
                        .append(options.depth.to_string()),
                ),
        )
        .build()
}

/// Build a `<subscribe/>` IQ with XEP-0497 options.
#[must_use]
pub fn subscribe_iq(
    service: &Jid,
    node: &str,
    own: &Jid,
    options: &ExtendedSubscription,
) -> Element {
    Element::builder("iq", xmpp_parsers::ns::JABBER_CLIENT)
        .attr("type", "set")
        .attr("to", service.to_bare().to_string())
        .attr("id", format!("pubsub-subscribe-{}", uuid::Uuid::new_v4()))
        .append(
            Element::builder("pubsub", NS_PUBSUB).append(
                Element::builder("subscribe", NS_PUBSUB)
                    .attr("node", node)
                    .attr("jid", own.to_bare().to_string())
                    .append(Element::builder("options", NS_PUBSUB).append(options_form(options))),
            ),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn builds_subscription_options() {
        let service: Jid = "pubsub.example.org".parse().expect("service");
        let own: Jid = "romeo@example.org".parse().expect("own");
        let options = ExtendedSubscription {
            types: vec![
                SubscriptionType::Items,
                SubscriptionType::LinkedItems,
                SubscriptionType::Metadata,
            ],
            depth: -1,
        };
        let iq = subscribe_iq(&service, "some-node-name", &own, &options);
        let subscribe = iq
            .get_child("pubsub", NS_PUBSUB)
            .and_then(|pubsub| pubsub.get_child("subscribe", NS_PUBSUB))
            .expect("subscribe");
        assert_eq!(subscribe.attr("node"), Some("some-node-name"));
        assert_eq!(subscribe.attr("jid"), Some("romeo@example.org"));

        let form = subscribe
            .get_child("options", NS_PUBSUB)
            .and_then(|options| options.get_child("x", crate::data_form::NS_DATA_FORMS))
            .expect("form");
        let values = |var: &str| {
            form.children()
                .find(|field| field.attr("var") == Some(var))
                .map(|field| {
                    field
                        .children()
                        .filter(|child| child.name() == "value")
                        .map(Element::text)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        assert_eq!(values("FORM_TYPE"), vec![NS_SUBSCRIBE_OPTIONS.to_string()]);
        assert_eq!(
            values(&format!("{{{NS_PUBSUB_EXT_SUB}}}type")),
            vec![
                "items".to_string(),
                "linked items".to_string(),
                "metadata".to_string()
            ]
        );
        assert_eq!(
            values(&format!("{{{NS_PUBSUB_EXT_SUB}}}depth")),
            vec!["-1".to_string()]
        );
    }

    #[test]
    fn default_options_only_follow_items() {
        let options = ExtendedSubscription::default();
        assert_eq!(options.types, vec![SubscriptionType::Items]);
        assert_eq!(options.depth, 0);

        let iq = subscribe_iq(
            &Jid::from_str("pubsub.example.org").expect("service"),
            "node",
            &Jid::from_str("romeo@example.org").expect("own"),
            &options,
        );
        assert!(iq
            .get_child("pubsub", NS_PUBSUB)
            .and_then(|pubsub| pubsub.get_child("subscribe", NS_PUBSUB))
            .and_then(|subscribe| subscribe.get_child("options", NS_PUBSUB))
            .is_some());
    }
}
