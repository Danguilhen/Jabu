//! Extended Channel Search (XEP-0433).
//!
//! A search service exposes a data form that is submitted inside a
//! `<search/>` element, and answers with RSM-paginated `<item/>` entries.

use jid::Jid;
use minidom::{Element, NSChoice};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const NS_CHANNEL_SEARCH: &str = "urn:xmpp:channel-search:0:search";
pub const NS_CHANNEL_SEARCH_ERROR: &str = "urn:xmpp:channel-search:0:error";
pub const NS_RSM: &str = "http://jabber.org/protocol/rsm";
pub const FORM_TYPE: &str = "urn:xmpp:channel-search:0:search-params";
pub const DEFAULT_SEARCH_SERVICE: &str = "search.jabber.network";
pub const SORT_ADDRESS: &str = "{urn:xmpp:channel-search:0:order}address";
pub const SORT_NUSERS: &str = "{urn:xmpp:channel-search:0:order}nusers";

/// One public channel returned by a search service.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChannelSearchItem {
    pub address: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub nusers: Option<u32>,
    #[serde(default)]
    pub service_type: Option<String>,
    #[serde(default)]
    pub is_open: bool,
    #[serde(default)]
    pub anonymity_mode: Option<String>,
}

/// One RSM page of channel search results.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChannelSearchPage {
    pub items: Vec<ChannelSearchItem>,
    pub first: Option<String>,
    pub last: Option<String>,
    pub count: Option<usize>,
}

impl ChannelSearchPage {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Ask a service for its search form.
#[must_use]
pub fn search_form_request(service: &Jid) -> Element {
    Element::builder("iq", "jabber:client")
        .attr("type", "get")
        .attr("to", service.to_bare().to_string())
        .attr("id", format!("channel-search-form-{}", Uuid::new_v4()))
        .append(Element::builder("search", NS_CHANNEL_SEARCH))
        .build()
}

/// Build a keyword search submission.
///
/// `after` is an RSM cursor obtained from a previous [`ChannelSearchPage`].
#[must_use]
pub fn search_query(service: &Jid, query: &str, max: usize, after: Option<&str>) -> Element {
    let form = Element::builder("x", "jabber:x:data")
        .attr("type", "submit")
        .append(
            Element::builder("field", "jabber:x:data")
                .attr("var", "FORM_TYPE")
                .attr("type", "hidden")
                .append(Element::builder("value", "jabber:x:data").append(FORM_TYPE)),
        )
        .append(
            Element::builder("field", "jabber:x:data")
                .attr("var", "q")
                .attr("type", "text-single")
                .append(Element::builder("value", "jabber:x:data").append(query.trim())),
        )
        .append(
            Element::builder("field", "jabber:x:data")
                .attr("var", "key")
                .attr("type", "list-single")
                .append(Element::builder("value", "jabber:x:data").append(SORT_ADDRESS)),
        );

    let mut set = Element::builder("set", NS_RSM);
    if max > 0 {
        set = set.append(Element::builder("max", NS_RSM).append(max.to_string()));
    }
    if let Some(after) = after.filter(|after| !after.is_empty()) {
        set = set.append(Element::builder("after", NS_RSM).append(after.to_string()));
    }
    let set = set.build();

    let mut search = Element::builder("search", NS_CHANNEL_SEARCH);
    if max > 0 || after.is_some() {
        search = search.append(set);
    }
    search = search.append(form);

    Element::builder("iq", "jabber:client")
        .attr("type", "get")
        .attr("to", service.to_bare().to_string())
        .attr("id", format!("channel-search-{}", Uuid::new_v4()))
        .append(search)
        .build()
}

/// Parse a `<result/>` payload returned by a search service.
#[must_use]
pub fn parse_results(result: &Element) -> ChannelSearchPage {
    let items = result
        .children()
        .filter(|child| child.name() == "item")
        .filter_map(parse_item)
        .collect();
    let (first, last, count) = result
        .get_child("set", NS_RSM)
        .map(|set| {
            (
                text_of(set, "first"),
                text_of(set, "last"),
                text_of(set, "count").and_then(|count| count.parse().ok()),
            )
        })
        .unwrap_or((None, None, None));
    ChannelSearchPage {
        items,
        first,
        last,
        count,
    }
}

fn parse_item(item: &Element) -> Option<ChannelSearchItem> {
    let address = item.attr("address")?.trim();
    if address.is_empty() {
        return None;
    }
    let is_open = item
        .get_child("is-open", NSChoice::Any)
        .map(|open| {
            let value = open.text();
            !matches!(value.trim(), "false" | "0")
        })
        .unwrap_or(false);
    Some(ChannelSearchItem {
        address: address.to_string(),
        name: text_of(item, "name"),
        description: text_of(item, "description"),
        language: text_of(item, "language"),
        nusers: text_of(item, "nusers").and_then(|nusers| nusers.parse().ok()),
        service_type: text_of(item, "service-type"),
        is_open,
        anonymity_mode: text_of(item, "anonymity-mode"),
    })
}

fn text_of(parent: &Element, name: &str) -> Option<String> {
    parent
        .get_child(name, NSChoice::Any)
        .map(|child| child.text().trim().to_string())
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_items_and_cursor() {
        let xml = r#"<result xmlns="urn:xmpp:channel-search:0:search">
            <item address="commteam@muc.xmpp.org">
                <name>commteam</name>
                <nusers>10</nusers>
                <is-open/>
            </item>
            <item address="operators@muc.xmpp.org">
                <name>XMPP Service Operators</name>
                <description>Operators of federated XMPP services</description>
                <language>en</language>
                <nusers>43</nusers>
                <service-type>xep-0045</service-type>
                <is-open>true</is-open>
                <anonymity-mode>muc_semianonymous</anonymity-mode>
            </item>
            <set xmlns="http://jabber.org/protocol/rsm">
                <first>opaque-1</first>
                <last>opaque-2</last>
                <count>2</count>
            </set>
        </result>"#;
        let page = parse_results(&Element::from_reader(xml.as_bytes()).expect("result"));

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].address, "commteam@muc.xmpp.org");
        assert_eq!(page.items[0].name.as_deref(), Some("commteam"));
        assert_eq!(page.items[0].nusers, Some(10));
        assert!(page.items[0].is_open);
        assert_eq!(
            page.items[1].description.as_deref(),
            Some("Operators of federated XMPP services")
        );
        assert_eq!(page.items[1].language.as_deref(), Some("en"));
        assert_eq!(page.items[1].service_type.as_deref(), Some("xep-0045"));
        assert_eq!(
            page.items[1].anonymity_mode.as_deref(),
            Some("muc_semianonymous")
        );
        assert_eq!(page.first.as_deref(), Some("opaque-1"));
        assert_eq!(page.last.as_deref(), Some("opaque-2"));
        assert_eq!(page.count, Some(2));
    }

    #[test]
    fn an_empty_result_is_not_an_error() {
        let xml = r#"<result xmlns="urn:xmpp:channel-search:0:search"/>"#;
        let page = parse_results(&Element::from_reader(xml.as_bytes()).expect("result"));
        assert!(page.is_empty());
        assert!(page.first.is_none());
    }

    #[test]
    fn query_carries_the_form_type_key_and_cursor() {
        let service: Jid = "search.example.org".parse().expect("service");
        let iq = search_query(&service, "xmpp", 25, Some("cursor-1"));
        assert_eq!(iq.attr("type"), Some("get"));
        assert_eq!(iq.attr("to"), Some("search.example.org"));
        let search = iq.get_child("search", NS_CHANNEL_SEARCH).expect("search");
        let set = search.get_child("set", NS_RSM).expect("rsm");
        assert_eq!(
            set.get_child("max", NS_RSM).map(|max| max.text()),
            Some("25".to_string())
        );
        assert_eq!(
            set.get_child("after", NS_RSM).map(|after| after.text()),
            Some("cursor-1".to_string())
        );
        let form = search.get_child("x", "jabber:x:data").expect("form");
        let values: Vec<(String, String)> = form
            .children()
            .filter(|child| child.name() == "field")
            .filter_map(|field| {
                let var = field.attr("var")?.to_string();
                let value = field
                    .get_child("value", NSChoice::Any)
                    .map(|value| value.text())?;
                Some((var, value))
            })
            .collect();
        assert!(values.contains(&("FORM_TYPE".to_string(), FORM_TYPE.to_string())));
        assert!(values.contains(&("q".to_string(), "xmpp".to_string())));
        assert!(values.contains(&("key".to_string(), SORT_ADDRESS.to_string())));
    }

    #[test]
    fn form_request_targets_the_service() {
        let service: Jid = "search.example.org".parse().expect("service");
        let iq = search_form_request(&service);
        assert_eq!(iq.attr("type"), Some("get"));
        assert!(iq.get_child("search", NS_CHANNEL_SEARCH).is_some());
    }
}
