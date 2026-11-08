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

/// Result Set Management (XEP-0059).
pub const NS_RSM: &str = "http://jabber.org/protocol/rsm";

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

/// A XEP-0499 extended `disco#items` request.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ExtendedDiscovery {
    pub node: Option<String>,
    pub include_items: bool,
    pub include_nodes: bool,
    pub linked_nodes: bool,
    pub full_metadata: bool,
    pub depth: u32,
    /// Maximum number of results, via XEP-0059 RSM.
    pub max: Option<u32>,
    /// Continue after this RSM cursor.
    pub after: Option<String>,
}

impl Default for ExtendedDiscovery {
    fn default() -> Self {
        Self {
            node: None,
            include_items: false,
            include_nodes: true,
            linked_nodes: true,
            full_metadata: false,
            depth: 0,
            max: None,
            after: None,
        }
    }
}

fn boolean_field(var: &str, value: bool) -> Element {
    data_field(var, "boolean", &[value.to_string()])
}

/// Build the XEP-0499 `disco#items` query.
///
/// # Errors
///
/// Returns an error when the request selects neither items nor nodes.
pub fn extended_items_query(request: &ExtendedDiscovery) -> Result<Element> {
    if !request.include_items && !request.include_nodes {
        return Err(XmppError::ProtocolError(
            "an extended discovery request must select items or nodes".into(),
        ));
    }
    let mut types = Vec::new();
    if request.include_items {
        types.push("items".to_string());
    }
    if request.include_nodes {
        types.push("nodes".to_string());
    }
    let form = Element::builder("x", crate::data_form::NS_DATA_FORMS)
        .attr("type", "submit")
        .append(data_field(
            "FORM_TYPE",
            "hidden",
            &[NS_PUBSUB_EXT_DISCO.to_string()],
        ))
        .append(data_field("type", "list-multi", &types))
        .append(boolean_field("linked_nodes", request.linked_nodes))
        .append(boolean_field("full_metadata", request.full_metadata))
        .append(data_field(
            "depth",
            "text-single",
            &[request.depth.to_string()],
        ));

    let mut query = Element::builder("query", NS_DISCO_ITEMS);
    if let Some(node) = request.node.as_deref().filter(|node| !node.is_empty()) {
        query = query.attr("node", node);
    }
    if let Some(set) = rsm_set(request) {
        query = query.append(set);
    }
    Ok(query.append(form).build())
}

fn rsm_set(request: &ExtendedDiscovery) -> Option<Element> {
    let after = request.after.as_deref().filter(|after| !after.is_empty());
    if request.max.is_none() && after.is_none() {
        return None;
    }
    let mut set = Element::builder("set", NS_RSM);
    if let Some(max) = request.max {
        set = set.append(Element::builder("max", NS_RSM).append(max.to_string()));
    }
    if let Some(after) = after {
        set = set.append(Element::builder("after", NS_RSM).append(after.to_string()));
    }
    Some(set.build())
}

/// One node returned by a XEP-0499 extended discovery query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoveredNode {
    pub jid: String,
    pub node: String,
    pub name: Option<String>,
    pub relationships: crate::pubsub::relationships::NodeRelationships,
}

/// One page of XEP-0499 results.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtendedDiscoveryPage {
    pub nodes: Vec<DiscoveredNode>,
    pub first: Option<String>,
    pub last: Option<String>,
    pub count: Option<usize>,
}

impl ExtendedDiscoveryPage {
    /// True when the service returned a cursor that can fetch more results.
    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.last.is_some()
    }
}

/// Parse the `disco#items` result of a XEP-0499 query.
#[must_use]
pub fn parse_extended_items(query: &Element) -> Vec<DiscoveredNode> {
    query
        .children()
        .filter(|child| child.name() == "item")
        .filter_map(|item| {
            let node = item.attr("node")?.to_string();
            let relationships = item
                .children()
                .find(|child| child.name() == "x" && child.ns() == crate::data_form::NS_DATA_FORMS)
                .map(crate::pubsub::relationships::from_form)
                .unwrap_or_default();
            Some(DiscoveredNode {
                jid: item.attr("jid").unwrap_or_default().to_string(),
                node,
                name: item.attr("name").map(str::to_string),
                relationships,
            })
        })
        .collect()
}

/// Parse a XEP-0499 page including its RSM cursor.
#[must_use]
pub fn parse_extended_page(query: &Element) -> ExtendedDiscoveryPage {
    let set = query.get_child("set", NS_RSM);
    let field = |name: &str| {
        set.and_then(|set| set.get_child(name, NS_RSM))
            .map(|element| element.text().trim().to_string())
            .filter(|text| !text.is_empty())
    };
    ExtendedDiscoveryPage {
        nodes: parse_extended_items(query),
        first: field("first"),
        last: field("last"),
        count: field("count").and_then(|count| count.parse::<usize>().ok()),
    }
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

    #[test]
    fn builds_an_extended_discovery_query() {
        let request = ExtendedDiscovery {
            node: Some("urn:xmpp:microblog:0".to_string()),
            include_nodes: true,
            linked_nodes: true,
            depth: 2,
            ..ExtendedDiscovery::default()
        };
        let query = extended_items_query(&request).expect("query");
        assert_eq!(query.attr("node"), Some("urn:xmpp:microblog:0"));
        let form = query
            .get_child("x", crate::data_form::NS_DATA_FORMS)
            .expect("form");
        let field_values = |var: &str| {
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
        assert_eq!(field_values("FORM_TYPE"), vec![NS_PUBSUB_EXT_DISCO]);
        assert_eq!(field_values("type"), vec!["nodes"]);
        assert_eq!(field_values("linked_nodes"), vec!["true"]);
        assert_eq!(field_values("depth"), vec!["2"]);
    }

    #[test]
    fn parses_extended_discovery_items_with_relationships() {
        let query = Element::from_str(
            r#"<query xmlns="http://jabber.org/protocol/disco#items">
                 <item jid="juliet@example.org" node="urn:xmpp:microblog:0:comments/a" name="Comments">
                   <x xmlns="jabber:x:data" type="result">
                     <field var="FORM_TYPE"><value>http://jabber.org/protocol/pubsub#meta-data</value></field>
                     <field var="{urn:xmpp:pubsub-relationships:0}parent"><value>urn:xmpp:microblog:0</value></field>
                   </x>
                 </item>
               </query>"#,
        )
        .expect("query");
        let nodes = parse_extended_items(&query);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node, "urn:xmpp:microblog:0:comments/a");
        assert_eq!(nodes[0].name.as_deref(), Some("Comments"));
        assert_eq!(
            nodes[0].relationships.parent.as_deref(),
            Some("urn:xmpp:microblog:0")
        );
    }

    #[test]
    fn adds_an_rsm_page_request() {
        let request = ExtendedDiscovery {
            node: Some("urn:xmpp:microblog:0".to_string()),
            max: Some(25),
            after: Some("cursor-2".to_string()),
            ..ExtendedDiscovery::default()
        };
        let query = extended_items_query(&request).expect("query");
        let set = query.get_child("set", NS_RSM).expect("rsm set");
        assert_eq!(
            set.get_child("max", NS_RSM).map(Element::text).as_deref(),
            Some("25")
        );
        assert_eq!(
            set.get_child("after", NS_RSM).map(Element::text).as_deref(),
            Some("cursor-2")
        );
    }

    #[test]
    fn parses_a_page_with_a_cursor() {
        let query = Element::from_str(
            r#"<query xmlns="http://jabber.org/protocol/disco#items">
                 <item jid="pubsub.example.org" node="a" name="A"/>
                 <item jid="pubsub.example.org" node="b" name="B"/>
                 <set xmlns="http://jabber.org/protocol/rsm">
                   <first>a</first><last>b</last><count>42</count>
                 </set>
               </query>"#,
        )
        .expect("query");
        let page = parse_extended_page(&query);
        assert_eq!(page.nodes.len(), 2);
        assert_eq!(page.first.as_deref(), Some("a"));
        assert_eq!(page.last.as_deref(), Some("b"));
        assert_eq!(page.count, Some(42));
        assert!(page.has_more());
    }
}
