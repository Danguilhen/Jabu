//! `PubSub` discovery extensions (XEP-0462 and XEP-0499).
//!
//! XEP-0462 adds an include/exclude filter for `disco#items` queries so a
//! client can ask only for nodes of a given `pubsub#type`. XEP-0499 extends
//! the same request with relationship discovery; both use a data form inside
//! the `disco#items` query.

use minidom::Element;

use crate::{Result, XmppError};

/// XEP-0462 namespace and advertised feature.
pub const NS_PUBSUB_FILTER: &str = "urn:xmpp:pubsub-filter:0";

/// XEP-0499 namespace and advertised feature.
pub const NS_PUBSUB_EXT_DISCO: &str = "urn:xmpp:pubsub-ext-disco:0";

/// XEP-0030 `disco#items` namespace.
pub const NS_DISCO_ITEMS: &str = "http://jabber.org/protocol/disco#items";

/// Include/exclude filter for `pubsub#type` (XEP-0462).
///
/// A request may set only one of the two lists; setting both is a protocol
/// error. An empty list means "do not filter".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeTypeFilter {
    pub included: Vec<String>,
    pub excluded: Vec<String>,
}

impl NodeTypeFilter {
    #[must_use]
    pub fn included(types: impl IntoIterator<Item = String>) -> Self {
        Self {
            included: types.into_iter().collect(),
            excluded: Vec::new(),
        }
    }

    #[must_use]
    pub fn excluded(types: impl IntoIterator<Item = String>) -> Self {
        Self {
            included: Vec::new(),
            excluded: types.into_iter().collect(),
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.included.is_empty() && self.excluded.is_empty()
    }

    #[must_use]
    pub fn matches(&self, node_type: &str) -> bool {
        if !self.included.is_empty() {
            return self.included.iter().any(|value| value == node_type);
        }
        if !self.excluded.is_empty() {
            return !self.excluded.iter().any(|value| value == node_type);
        }
        true
    }
}

fn data_field(var: &str, kind: &str, values: &[String]) -> Element {
    let mut field = Element::builder("field", crate::data_form::NS_DATA_FORMS)
        .attr("var", var)
        .attr("type", kind);
    for value in values {
        field = field.append(
            Element::builder("value", crate::data_form::NS_DATA_FORMS).append(value.clone()),
        );
    }
    field.build()
}

/// Build the `<filter/>` child for a `disco#items` query (XEP-0462).
///
/// # Errors
///
/// Returns an error when both `included` and `excluded` are set.
pub fn filter_element(filter: &NodeTypeFilter) -> Result<Element> {
    if !filter.included.is_empty() && !filter.excluded.is_empty() {
        return Err(XmppError::ProtocolError(
            "a node type filter cannot include and exclude types at once".into(),
        ));
    }
    let var = if filter.included.is_empty() {
        "excluded-types"
    } else {
        "included-types"
    };
    let values = if filter.included.is_empty() {
        &filter.excluded
    } else {
        &filter.included
    };
    let form = Element::builder("x", crate::data_form::NS_DATA_FORMS)
        .attr("type", "submit")
        .append(data_field(
            "FORM_TYPE",
            "hidden",
            &[NS_PUBSUB_FILTER.to_string()],
        ))
        .append(data_field(var, "list-multi", values));
    Ok(Element::builder("filter", NS_PUBSUB_FILTER)
        .append(form)
        .build())
}

/// Parse a `<filter/>` element back into a [`NodeTypeFilter`].
#[must_use]
pub fn parse_filter(filter: &Element) -> Option<NodeTypeFilter> {
    if filter.name() != "filter" || filter.ns() != NS_PUBSUB_FILTER {
        return None;
    }
    let form = filter.get_child("x", crate::data_form::NS_DATA_FORMS)?;
    let mut included = Vec::new();
    let mut excluded = Vec::new();
    let mut form_type = None;
    for field in form.children().filter(|child| child.name() == "field") {
        let Some(var) = field.attr("var") else {
            continue;
        };
        let values: Vec<String> = field
            .children()
            .filter(|child| child.name() == "value")
            .map(|value| value.text().trim().to_string())
            .collect();
        match var {
            "FORM_TYPE" => form_type = values.first().cloned(),
            "included-types" => included = values,
            "excluded-types" => excluded = values,
            _ => {}
        }
    }
    if form_type.as_deref() != Some(NS_PUBSUB_FILTER) {
        return None;
    }
    if !included.is_empty() && !excluded.is_empty() {
        return None;
    }
    Some(NodeTypeFilter { included, excluded })
}

/// Build a `disco#items` query, optionally carrying a node type filter.
///
/// # Errors
///
/// Returns an error when the filter is invalid.
pub fn items_query(filter: Option<&NodeTypeFilter>) -> Result<Element> {
    let mut query = Element::builder("query", NS_DISCO_ITEMS);
    if let Some(filter) = filter.filter(|filter| !filter.is_empty()) {
        query = query.append(filter_element(filter)?);
    }
    Ok(query.build())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn builds_an_include_filter() {
        let filter = NodeTypeFilter::included(vec!["urn:xmpp:spaces:0".to_string()]);
        let element = filter_element(&filter).expect("filter");
        assert_eq!(element.name(), "filter");
        assert_eq!(element.ns(), NS_PUBSUB_FILTER);
        let form = element
            .get_child("x", crate::data_form::NS_DATA_FORMS)
            .expect("form");
        assert_eq!(form.attr("type"), Some("submit"));
        let fields: Vec<_> = form
            .children()
            .filter(|child| child.name() == "field")
            .collect();
        assert_eq!(fields[0].attr("var"), Some("FORM_TYPE"));
        assert_eq!(fields[1].attr("var"), Some("included-types"));
        assert_eq!(fields[1].attr("type"), Some("list-multi"));
        assert_eq!(
            fields[1]
                .get_child("value", crate::data_form::NS_DATA_FORMS)
                .map(Element::text)
                .as_deref(),
            Some("urn:xmpp:spaces:0")
        );
    }

    #[test]
    fn round_trips_the_filter() {
        let original =
            NodeTypeFilter::excluded(vec!["urn:xmpp:microblog:0".to_string(), String::new()]);
        let element = filter_element(&original).expect("filter");
        let parsed = parse_filter(&element).expect("parsed");
        assert_eq!(parsed, original);
    }

    #[test]
    fn rejects_include_and_exclude_together() {
        let filter = NodeTypeFilter {
            included: vec!["a".to_string()],
            excluded: vec!["b".to_string()],
        };
        assert!(filter_element(&filter).is_err());
    }

    #[test]
    fn ignores_other_forms() {
        let element = Element::from_str(
            r#"<filter xmlns="urn:xmpp:pubsub-filter:0"><x xmlns="jabber:x:data" type="submit"><field var="FORM_TYPE"><value>other</value></field></x></filter>"#,
        )
        .expect("xml");
        assert!(parse_filter(&element).is_none());
    }

    #[test]
    fn omits_the_filter_when_empty() {
        let query = items_query(Some(&NodeTypeFilter::default())).expect("query");
        assert_eq!(query.name(), "query");
        assert!(query.children().next().is_none());
    }
}
