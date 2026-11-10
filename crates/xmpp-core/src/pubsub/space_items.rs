//! Space room-list items and their XEP-0059 pages.
//!
//! A XEP-0503 space stores its rooms, directories and images as items in one
//! `PubSub` node. Large spaces can exceed a server's item limit, so the list is
//! fetched in pages and merged into one [`SpaceItems`] value.

use jid::Jid;
use minidom::Element;
use xmpp_parsers::ns;

use crate::pubsub::rsm::{PageInfo, PageRequest};
use crate::spaces::{SpaceDirectory, SpaceRoom};

const NS_PUBSUB: &str = "http://jabber.org/protocol/pubsub";
const NS_AVATAR_METADATA: &str = "urn:xmpp:avatar:metadata";

/// The parts of a space node fetched as items.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpaceItems {
    pub rooms: Vec<SpaceRoom>,
    pub directories: Vec<SpaceDirectory>,
    pub avatar: Option<String>,
    pub banner: Option<String>,
}

impl SpaceItems {
    /// Merge one page into this collection, dropping rooms already seen.
    ///
    /// Image and directory items are single-item nodes, so a page that does
    /// not mention them must not erase what an earlier page returned.
    pub fn merge(&mut self, page: Self) {
        let mut seen: std::collections::HashSet<String> = self
            .rooms
            .iter()
            .map(|room| room.conference.clone())
            .collect();
        for room in page.rooms {
            if seen.insert(room.conference.clone()) {
                self.rooms.push(room);
            }
        }
        if !page.directories.is_empty() {
            self.directories = page.directories;
        }
        if page.avatar.is_some() {
            self.avatar = page.avatar;
        }
        if page.banner.is_some() {
            self.banner = page.banner;
        }
        self.rooms.sort_by_key(|room: &SpaceRoom| room.weight);
    }
}

/// One page of a space's item list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpaceItemsPage {
    pub items: SpaceItems,
    pub page: PageInfo,
}

/// Build a `pubsub#items` request for one page of a space node.
#[must_use]
pub fn items_iq(service: &Jid, node: &str, request: &PageRequest) -> Element {
    let items = request.apply(
        Element::builder("items", NS_PUBSUB)
            .attr("node", node.to_string())
            .build(),
    );
    Element::builder("iq", ns::JABBER_CLIENT)
        .attr("type", "get")
        .attr("to", service.to_bare().to_string())
        .attr("id", format!("space-items-{}", uuid::Uuid::new_v4()))
        .append(Element::builder("pubsub", NS_PUBSUB).append(items))
        .build()
}

/// Parse one `<items/>` result into rooms, metadata and a cursor.
#[must_use]
pub fn parse(items: &Element) -> SpaceItemsPage {
    let mut parsed = SpaceItems::default();
    for item in items.children().filter(|child| child.name() == "item") {
        let item_id = item.attr("id").unwrap_or_default();
        if item_id == crate::spaces::NS_SPACE_AVATAR || item_id == crate::spaces::NS_SPACE_BANNER {
            let url = item
                .get_child("metadata", NS_AVATAR_METADATA)
                .and_then(|metadata| metadata.get_child("info", NS_AVATAR_METADATA))
                .and_then(|info| info.attr("url"))
                .map(str::to_string);
            if item_id == crate::spaces::NS_SPACE_AVATAR {
                parsed.avatar = url;
            } else {
                parsed.banner = url;
            }
            continue;
        }
        if let Some(node) = item.get_child("directories", crate::spaces::NS_SPACE_DIRECTORIES) {
            parsed.directories = crate::parse_space_directories(node);
            continue;
        }
        if let Some(conference) = item.get_child("conference", crate::spaces::NS_BOOKMARKS) {
            if let Some(room) = crate::parse_space_room(item_id, conference) {
                parsed.rooms.push(room);
            }
        }
    }
    parsed.rooms.sort_by_key(|room: &SpaceRoom| room.weight);
    SpaceItemsPage {
        items: parsed,
        page: PageInfo::parse(items),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn builds_a_paged_items_request() {
        let service: Jid = "spaces.example.org".parse().expect("service");
        let iq = items_iq(&service, "team", &PageRequest::new(25, Some("room-2")));
        assert_eq!(iq.attr("to"), Some("spaces.example.org"));
        let items = iq
            .get_child("pubsub", NS_PUBSUB)
            .and_then(|pubsub| pubsub.get_child("items", NS_PUBSUB))
            .expect("items");
        assert_eq!(items.attr("node"), Some("team"));
        let set = items
            .get_child("set", crate::pubsub::rsm::NS_RSM)
            .expect("set");
        assert_eq!(
            set.get_child("max", crate::pubsub::rsm::NS_RSM)
                .map(Element::text)
                .as_deref(),
            Some("25")
        );
        assert_eq!(
            set.get_child("after", crate::pubsub::rsm::NS_RSM)
                .map(Element::text)
                .as_deref(),
            Some("room-2")
        );
    }

    #[test]
    fn parses_rooms_metadata_and_the_cursor() {
        let items = Element::from_str(
            r#"<items xmlns="http://jabber.org/protocol/pubsub">
                 <item id="lounge@conference.example.org">
                   <conference xmlns="urn:xmpp:bookmarks:1" name="Lounge" autojoin="true">
                     <nick>romeo</nick>
                     <extensions>
                       <hierarchy xmlns="urn:movim:spaces:0" weight="2"/>
                     </extensions>
                   </conference>
                 </item>
                 <item id="urn:xmpp:spaces:avatar:metadata:0">
                   <metadata xmlns="urn:xmpp:avatar:metadata">
                     <info url="https://example.org/avatar.png"/>
                   </metadata>
                 </item>
                 <item id="directories">
                   <directories xmlns="https://slidge.im/spaces/bookmarks-hierarchy">
                     <directory id="news" title="News"/>
                   </directories>
                 </item>
                 <item id="urn:xmpp:spaces:banner:metadata:0">
                   <metadata xmlns="urn:xmpp:avatar:metadata">
                     <info url="https://example.org/banner.png"/>
                   </metadata>
                 </item>
                 <set xmlns="http://jabber.org/protocol/rsm">
                   <first>lounge@conference.example.org</first>
                   <last>urn:xmpp:spaces:banner:metadata:0</last>
                   <count>120</count>
                 </set>
               </items>"#,
        )
        .expect("items");
        let page = parse(&items);
        assert_eq!(page.items.rooms.len(), 1);
        assert_eq!(
            page.items.rooms[0].conference,
            "lounge@conference.example.org"
        );
        assert_eq!(page.items.rooms[0].name, "Lounge");
        assert!(page.items.rooms[0].autojoin);
        assert_eq!(page.items.directories.len(), 1);
        assert_eq!(page.items.directories[0].id, "news");
        assert_eq!(
            page.items.avatar.as_deref(),
            Some("https://example.org/avatar.png")
        );
        assert_eq!(
            page.items.banner.as_deref(),
            Some("https://example.org/banner.png")
        );
        assert_eq!(page.page.count, Some(120));
        assert!(page.page.has_more());
    }

    #[test]
    fn merges_pages_without_duplicating_rooms_or_losing_metadata() {
        let first = SpaceItems {
            rooms: vec![SpaceRoom {
                conference: "a@conference.example.org".to_string(),
                name: "A".to_string(),
                weight: 2,
                ..SpaceRoom::default()
            }],
            directories: vec![SpaceDirectory {
                id: "news".to_string(),
                name: "News".to_string(),
                order: 0,
            }],
            avatar: Some("https://example.org/avatar.png".to_string()),
            banner: None,
        };
        let second = SpaceItems {
            rooms: vec![
                SpaceRoom {
                    conference: "b@conference.example.org".to_string(),
                    name: "B".to_string(),
                    weight: 1,
                    ..SpaceRoom::default()
                },
                SpaceRoom {
                    conference: "a@conference.example.org".to_string(),
                    name: "Duplicate A".to_string(),
                    weight: 3,
                    ..SpaceRoom::default()
                },
            ],
            ..SpaceItems::default()
        };

        let mut merged = first;
        merged.merge(second);
        assert_eq!(merged.rooms.len(), 2);
        assert_eq!(merged.rooms[0].conference, "b@conference.example.org");
        assert_eq!(merged.rooms[1].conference, "a@conference.example.org");
        assert_eq!(merged.directories[0].id, "news");
        assert_eq!(
            merged.avatar.as_deref(),
            Some("https://example.org/avatar.png")
        );
    }
}
