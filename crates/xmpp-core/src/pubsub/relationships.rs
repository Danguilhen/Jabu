//! `PubSub` node relationships (XEP-0496).
//!
//! Relationships are carried in the `pubsub#meta-data` form returned by a
//! node `disco#info`, and set through the node configuration form. A `parent`
//! link is hierarchical (comments belong to a post); a `link` is a plain
//! association.

use minidom::Element;
use serde::{Deserialize, Serialize};

/// XEP-0496 namespace and advertised feature.
pub const NS_PUBSUB_RELATIONSHIPS: &str = "urn:xmpp:pubsub-relationships:0";

/// Configuration/metadata field for the parent node.
pub const FIELD_PARENT: &str = "{urn:xmpp:pubsub-relationships:0}parent";

/// Configuration/metadata field for linked nodes.
pub const FIELD_LINK: &str = "{urn:xmpp:pubsub-relationships:0}link";

const NS_METADATA: &str = "http://jabber.org/protocol/pubsub#meta-data";
const NS_NODE_CONFIG: &str = "http://jabber.org/protocol/pubsub#node_config";

/// Relationships declared by one node.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRelationships {
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub links: Vec<String>,
}

impl NodeRelationships {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.parent.is_none() && self.links.is_empty()
    }
}

/// Read relationships from a `pubsub#meta-data` or `pubsub#node_config` form.
#[must_use]
pub fn from_form(form: &Element) -> NodeRelationships {
    let mut form_type = None;
    let mut parent = None;
    let mut links = Vec::new();
    for field in form.children().filter(|child| child.name() == "field") {
        let Some(var) = field.attr("var") else {
            continue;
        };
        let values: Vec<String> = field
            .children()
            .filter(|child| child.name() == "value")
            .map(|value| value.text().trim().to_string())
            .filter(|value| !value.is_empty())
            .collect();
        match var {
            "FORM_TYPE" => form_type = values.first().cloned(),
            FIELD_PARENT => parent = values.into_iter().next(),
            FIELD_LINK => links = values,
            _ => {}
        }
    }
    if !matches!(form_type.as_deref(), Some(NS_METADATA | NS_NODE_CONFIG)) {
        return NodeRelationships::default();
    }
    NodeRelationships { parent, links }
}

/// Build the node configuration field that declares a parent node.
#[must_use]
pub fn parent_config_field(parent: &str) -> Element {
    Element::builder("field", crate::data_form::NS_DATA_FORMS)
        .attr("var", FIELD_PARENT)
        .append(
            Element::builder("value", crate::data_form::NS_DATA_FORMS).append(parent.to_string()),
        )
        .build()
}

/// Build the node configuration field that declares linked nodes.
#[must_use]
pub fn links_config_field(links: &[String]) -> Element {
    let mut field = Element::builder("field", crate::data_form::NS_DATA_FORMS)
        .attr("var", FIELD_LINK)
        .attr("type", "list-multi");
    for link in links {
        field = field.append(
            Element::builder("value", crate::data_form::NS_DATA_FORMS).append(link.clone()),
        );
    }
    field.build()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn parses_metadata_relationships() {
        let form = Element::from_str(
            r#"<x xmlns="jabber:x:data" type="result">
                 <field var="FORM_TYPE"><value>http://jabber.org/protocol/pubsub#meta-data</value></field>
                 <field var="{urn:xmpp:pubsub-relationships:0}parent"><value>urn:xmpp:microblog:0</value></field>
                 <field var="{urn:xmpp:pubsub-relationships:0}link"><value>urn:xmpp:microblog:0:comments/a</value><value>urn:xmpp:microblog:0:comments/b</value></field>
               </x>"#,
        )
        .expect("form");
        let relationships = from_form(&form);
        assert_eq!(
            relationships.parent.as_deref(),
            Some("urn:xmpp:microblog:0")
        );
        assert_eq!(
            relationships.links,
            vec![
                "urn:xmpp:microblog:0:comments/a".to_string(),
                "urn:xmpp:microblog:0:comments/b".to_string()
            ]
        );
        assert!(!relationships.is_empty());
    }

    #[test]
    fn ignores_other_forms() {
        let form = Element::from_str(
            r#"<x xmlns="jabber:x:data" type="result"><field var="FORM_TYPE"><value>urn:xmpp:spaces:0</value></field><field var="{urn:xmpp:pubsub-relationships:0}parent"><value>a</value></field></x>"#,
        )
        .expect("form");
        assert!(from_form(&form).is_empty());
    }

    #[test]
    fn builds_config_fields() {
        let parent = parent_config_field("urn:xmpp:microblog:0");
        assert_eq!(parent.name(), "field");
        assert_eq!(parent.attr("var"), Some(FIELD_PARENT));
        assert_eq!(
            parent
                .get_child("value", crate::data_form::NS_DATA_FORMS)
                .map(Element::text)
                .as_deref(),
            Some("urn:xmpp:microblog:0")
        );

        let links = links_config_field(&["a".to_string(), "b".to_string()]);
        assert_eq!(links.attr("type"), Some("list-multi"));
        assert_eq!(
            links
                .children()
                .filter(|child| child.name() == "value")
                .count(),
            2
        );
    }
}
