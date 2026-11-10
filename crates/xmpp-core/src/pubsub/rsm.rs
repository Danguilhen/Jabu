//! Result Set Management (XEP-0059) for `PubSub` item lists.
//!
//! `PubSub` item queries take a `<set/>` child with a page size and a cursor:
//!
//! ```xml
//! <items node="...">
//!   <set xmlns="http://jabber.org/protocol/rsm">
//!     <max>100</max>
//!     <after>item-42</after>
//!   </set>
//! </items>
//! ```
//!
//! The reply repeats the `<set/>` with `<first/>`, `<last/>` and `<count/>`.
//! This module builds the request and parses the reply for the collections
//! that can grow without a practical bound: space room lists and XEP-0330
//! subscription lists.

use minidom::Element;

/// XEP-0059 namespace.
pub const NS_RSM: &str = "http://jabber.org/protocol/rsm";

/// A request for one page of a `PubSub` item list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageRequest {
    /// Maximum number of items to return.
    pub max: Option<u32>,
    /// Continue after this item id.
    pub after: Option<String>,
}

impl PageRequest {
    /// Request `max` items after an optional cursor.
    #[must_use]
    pub fn new(max: u32, after: Option<&str>) -> Self {
        Self {
            max: Some(max),
            after: after.filter(|after| !after.is_empty()).map(str::to_string),
        }
    }

    /// Build a request from optional parts.
    #[must_use]
    pub fn from_parts(max: Option<u32>, after: Option<&str>) -> Self {
        Self {
            max,
            after: after.filter(|after| !after.is_empty()).map(str::to_string),
        }
    }

    /// The `<set/>` element, or `None` when no paging was requested.
    #[must_use]
    pub fn element(&self) -> Option<Element> {
        if self.max.is_none() && self.after.is_none() {
            return None;
        }
        let mut set = Element::builder("set", NS_RSM);
        if let Some(max) = self.max {
            set = set.append(Element::builder("max", NS_RSM).append(max.to_string()));
        }
        if let Some(after) = self.after.as_deref() {
            set = set.append(Element::builder("after", NS_RSM).append(after.to_string()));
        }
        Some(set.build())
    }

    /// Append the `<set/>` element to an item list when paging is requested.
    #[must_use]
    pub fn apply(&self, mut parent: Element) -> Element {
        if let Some(set) = self.element() {
            parent.append_child(set);
        }
        parent
    }
}

/// The cursor returned with a page of items.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageInfo {
    /// Id of the first item on the page.
    pub first: Option<String>,
    /// Id of the last item on the page, usable as the next `after` cursor.
    pub last: Option<String>,
    /// Total number of items in the collection, when the service reports it.
    pub count: Option<usize>,
}

impl PageInfo {
    /// Parse the `<set/>` child of an `<items/>` element.
    #[must_use]
    pub fn parse(parent: &Element) -> Self {
        let set = parent.get_child("set", NS_RSM);
        let field = |name: &str| {
            set.and_then(|set| set.get_child(name, NS_RSM))
                .map(|element| element.text().trim().to_string())
                .filter(|text| !text.is_empty())
        };
        Self {
            first: field("first"),
            last: field("last"),
            count: field("count").and_then(|count| count.parse::<usize>().ok()),
        }
    }

    /// True when the service returned a cursor that can fetch more items.
    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.last.is_some()
    }

    /// The cursor to pass as `after` for the next page.
    #[must_use]
    pub fn next_after(&self) -> Option<&str> {
        self.last.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn builds_a_page_request() {
        let request = PageRequest::new(100, Some("item-42"));
        let set = request.element().expect("set");
        assert_eq!(set.name(), "set");
        assert_eq!(set.ns(), NS_RSM);
        assert_eq!(
            set.get_child("max", NS_RSM).map(Element::text).as_deref(),
            Some("100")
        );
        assert_eq!(
            set.get_child("after", NS_RSM).map(Element::text).as_deref(),
            Some("item-42")
        );
    }

    #[test]
    fn omits_an_empty_cursor_and_empty_request() {
        let request = PageRequest::new(25, Some(""));
        let set = request.element().expect("set");
        assert!(set.get_child("after", NS_RSM).is_none());
        assert!(PageRequest::default().element().is_none());
    }

    #[test]
    fn applies_the_set_to_an_item_list() {
        let request = PageRequest::new(10, None);
        let items =
            request.apply(Element::builder("items", "http://jabber.org/protocol/pubsub").build());
        assert_eq!(items.name(), "items");
        assert!(items.get_child("set", NS_RSM).is_some());
    }

    #[test]
    fn parses_a_page_cursor() {
        let items = Element::from_str(
            r#"<items xmlns="http://jabber.org/protocol/pubsub">
                 <item id="a"/>
                 <item id="b"/>
                 <set xmlns="http://jabber.org/protocol/rsm">
                   <first>a</first><last>b</last><count>42</count>
                 </set>
               </items>"#,
        )
        .expect("items");
        let page = PageInfo::parse(&items);
        assert_eq!(page.first.as_deref(), Some("a"));
        assert_eq!(page.last.as_deref(), Some("b"));
        assert_eq!(page.count, Some(42));
        assert!(page.has_more());
        assert_eq!(page.next_after(), Some("b"));
    }

    #[test]
    fn a_page_without_a_set_has_no_cursor() {
        let items = Element::from_str(
            r#"<items xmlns="http://jabber.org/protocol/pubsub"><item id="a"/></items>"#,
        )
        .expect("items");
        let page = PageInfo::parse(&items);
        assert_eq!(page, PageInfo::default());
        assert!(!page.has_more());
        assert_eq!(page.next_after(), None);
    }
}
