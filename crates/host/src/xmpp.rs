//! XMPP-backed public content source.
//!
//! The Host is a headless XMPP client: it connects with its own account and
//! fetches public `PubSub` nodes on demand. Only IQ results are routed; chat,
//! presence and encryption managers are not needed for public pages.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Utc;
use jid::Jid;
use minidom::{Element, NSChoice};
use xmpp_core::{
    microblog::{is_active_story, parse_item_for_node, MICROBLOG_NODE, STORIES_NODE},
    spaces::{NS_BOOKMARKS, NS_SPACE_DIRECTORIES, NS_SPACE_VOICE},
    Connection, ConnectionConfig, ConnectionEvent, IqRouter, MediaAttachment, PubSubItem,
};

use crate::content::{
    BlogProfile, CommunityPage, ContactProfile, ContentError, ContentSource, MediaLink, Post,
    SpacePage, SpaceRoomLink, Story, SubscriptionLink,
};

const NS_DATA_FORMS: &str = "jabber:x:data";

const NS_PUBSUB: &str = "http://jabber.org/protocol/pubsub";
const NS_RSM: &str = "http://jabber.org/protocol/rsm";
const NS_VCARD: &str = "vcard-temp";
const NS_CLIENT: &str = "jabber:client";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Public content read from XMPP `PubSub`.
pub struct XmppContent {
    connection: Arc<Connection>,
    iq: Arc<IqRouter>,
}

impl XmppContent {
    /// Connect the Host account and start routing IQ replies.
    ///
    /// # Errors
    ///
    /// Returns an error when the account cannot connect within the timeout.
    pub async fn connect(config: ConnectionConfig) -> Result<Self> {
        let (connection, mut events) = Connection::new(config);
        let connection = Arc::new(connection);
        let iq = Arc::new(IqRouter::new(Duration::from_secs(10)));
        let router = iq.clone();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                route_iq(event, &router);
            }
        });

        tokio::time::timeout(CONNECT_TIMEOUT, connection.connect())
            .await
            .map_err(|_| anyhow!("XMPP connection timed out"))?
            .map_err(|error| anyhow!("XMPP connection failed: {error}"))?;
        Ok(Self { connection, iq })
    }

    async fn request(&self, iq: Element) -> Result<Element, ContentError> {
        self.iq
            .request(|stanza| self.connection.send_stanza_now(stanza.clone()), iq)
            .await
            .map_err(|error| classify_error(&error.to_string()))
    }

    async fn fetch_raw_items(
        &self,
        jid: &Jid,
        node: &str,
        max: u32,
    ) -> Result<Vec<Element>, ContentError> {
        let iq = Element::builder("iq", NS_CLIENT);
        let iq = iq
            .attr("type", "get")
            .attr("to", jid.to_bare().to_string())
            .attr("id", format!("host-node-{}", uuid::Uuid::new_v4()))
            .append(
                Element::builder("pubsub", NS_PUBSUB).append(
                    Element::builder("items", NS_PUBSUB)
                        .attr("node", node)
                        .append(
                            Element::builder("set", NS_RSM)
                                .append(Element::builder("max", NS_RSM).append(max.to_string())),
                        ),
                ),
            )
            .build();
        let result = self.request(iq).await?;
        let Some(items) = result
            .get_child("pubsub", NS_PUBSUB)
            .and_then(|pubsub| pubsub.get_child("items", NS_PUBSUB))
        else {
            return Err(ContentError::NotFound);
        };
        Ok(items
            .children()
            .filter(|child| child.name() == "item")
            .cloned()
            .collect())
    }

    async fn fetch_posts(
        &self,
        jid: &Jid,
        node: &str,
        max: u32,
    ) -> Result<Vec<Post>, ContentError> {
        Ok(self
            .fetch_raw_items(jid, node, max)
            .await?
            .iter()
            .filter_map(|item| parse_item_for_node(item, jid, node))
            .map(post_from_item)
            .collect())
    }

    async fn fetch_node_metadata(
        &self,
        service: &Jid,
        node: &str,
    ) -> (Option<String>, Option<String>) {
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("to", service.to_bare().to_string())
            .attr("id", format!("host-disco-{}", uuid::Uuid::new_v4()))
            .append(
                Element::builder("query", "http://jabber.org/protocol/disco#info")
                    .attr("node", node),
            )
            .build();
        let Ok(result) = self.request(iq).await else {
            return (None, None);
        };
        let Some(query) = result.get_child("query", "http://jabber.org/protocol/disco#info") else {
            return (None, None);
        };
        let Some(form) = query
            .children()
            .find(|child| child.name() == "x" && child.ns() == NS_DATA_FORMS)
        else {
            return (None, None);
        };
        (
            form_value(form, "pubsub#title"),
            form_value(form, "pubsub#description"),
        )
    }

    async fn fetch_vcard(&self, jid: &Jid) -> Option<Element> {
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("to", jid.to_bare().to_string())
            .attr("id", format!("host-vcard-{}", uuid::Uuid::new_v4()))
            .append(Element::builder("vCard", NS_VCARD))
            .build();
        self.request(iq)
            .await
            .ok()?
            .get_child("vCard", NS_VCARD)
            .cloned()
    }

    async fn fetch_subscriptions(&self, jid: &Jid) -> Vec<SubscriptionLink> {
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("to", jid.to_bare().to_string())
            .attr("id", format!("host-subs-{}", uuid::Uuid::new_v4()))
            .append(
                Element::builder("pubsub", NS_PUBSUB).append(
                    Element::builder("items", NS_PUBSUB)
                        .attr("node", xmpp_core::pubsub::subscriptions::NS_SUBSCRIPTION),
                ),
            )
            .build();
        let Ok(result) = self.request(iq).await else {
            return Vec::new();
        };
        let Some(items) = result
            .get_child("pubsub", NS_PUBSUB)
            .and_then(|pubsub| pubsub.get_child("items", NS_PUBSUB))
        else {
            return Vec::new();
        };
        xmpp_core::pubsub::subscriptions::parse_items(items)
            .into_iter()
            .map(|entry| SubscriptionLink {
                server: entry.server,
                node: entry.node,
                title: entry.title,
            })
            .collect()
    }
}

#[async_trait]
impl ContentSource for XmppContent {
    async fn blog(&self, jid: &Jid) -> Result<BlogProfile, ContentError> {
        let bare = Jid::from(jid.to_bare());
        let posts = self.fetch_posts(&bare, MICROBLOG_NODE, 20).await?;
        let vcard = self.fetch_vcard(&bare).await;
        let name = vcard
            .as_ref()
            .and_then(|card| card.get_child("FN", NSChoice::Any))
            .map(Element::text)
            .filter(|name| !name.is_empty());
        let description = vcard
            .as_ref()
            .and_then(|card| card.get_child("DESC", NSChoice::Any))
            .map(Element::text)
            .filter(|description| !description.is_empty());
        Ok(BlogProfile {
            jid: bare.to_string(),
            name,
            description,
            posts,
        })
    }

    async fn post(&self, jid: &Jid, item_id: &str) -> Result<Post, ContentError> {
        let bare = Jid::from(jid.to_bare());
        self.fetch_posts(&bare, MICROBLOG_NODE, 100)
            .await?
            .into_iter()
            .find(|post| post.id == item_id)
            .ok_or(ContentError::NotFound)
    }

    async fn community(&self, service: &Jid, node: &str) -> Result<CommunityPage, ContentError> {
        let service = Jid::from(service.to_bare());
        let posts = self.fetch_posts(&service, node, 20).await?;
        let (title, description) = self.fetch_node_metadata(&service, node).await;
        Ok(CommunityPage {
            service: service.to_string(),
            node: node.to_string(),
            title,
            description,
            posts,
        })
    }

    async fn community_post(
        &self,
        service: &Jid,
        node: &str,
        item_id: &str,
    ) -> Result<Post, ContentError> {
        let service = Jid::from(service.to_bare());
        self.fetch_posts(&service, node, 100)
            .await?
            .into_iter()
            .find(|post| post.id == item_id)
            .ok_or(ContentError::NotFound)
    }

    async fn space(&self, service: &Jid, node: &str) -> Result<SpacePage, ContentError> {
        let service = Jid::from(service.to_bare());
        let items = self.fetch_raw_items(&service, node, 100).await?;
        let (title, description) = self.fetch_node_metadata(&service, node).await;
        let mut rooms = Vec::new();
        for item in items {
            let Some(conference) = item.get_child("conference", NS_BOOKMARKS) else {
                continue;
            };
            let jid = item
                .attr("id")
                .or_else(|| conference.attr("jid"))
                .unwrap_or_default()
                .to_string();
            if jid.is_empty() {
                continue;
            }
            let name = conference
                .attr("name")
                .filter(|name| !name.is_empty())
                .unwrap_or(&jid)
                .to_string();
            let extensions = conference.get_child("extensions", NS_BOOKMARKS);
            let directory = extensions
                .and_then(|extensions| extensions.get_child("hierarchy", NS_SPACE_DIRECTORIES))
                .and_then(|hierarchy| hierarchy.attr("directory-id"))
                .map(str::to_string);
            let voice = extensions
                .is_some_and(|extensions| extensions.get_child("voice", NS_SPACE_VOICE).is_some());
            rooms.push(SpaceRoomLink {
                jid,
                name,
                directory,
                voice,
            });
        }
        Ok(SpacePage {
            service: service.to_string(),
            node: node.to_string(),
            title,
            description,
            rooms,
        })
    }

    async fn story(&self, jid: &Jid, item_id: &str) -> Result<Story, ContentError> {
        let bare = Jid::from(jid.to_bare());
        let item = self
            .fetch_raw_items(&bare, STORIES_NODE, 50)
            .await?
            .iter()
            .filter_map(|item| parse_item_for_node(item, &bare, STORIES_NODE))
            .find(|item| item.id == item_id)
            .ok_or(ContentError::NotFound)?;
        if !is_active_story(&item, Utc::now()) {
            return Err(ContentError::NotFound);
        }
        Ok(Story {
            id: item.id,
            author_jid: item.publisher.to_bare().to_string(),
            caption: item.body.unwrap_or_default(),
            media: item.media.into_iter().next().map(media_link),
            published: item.published_at,
            expires: item.expires_at,
        })
    }

    async fn contact(&self, jid: &Jid) -> Result<ContactProfile, ContentError> {
        let bare = Jid::from(jid.to_bare());
        let vcard = self.fetch_vcard(&bare).await;
        let name = vcard
            .as_ref()
            .and_then(|card| card.get_child("FN", NSChoice::Any))
            .map(Element::text)
            .filter(|name| !name.is_empty());
        let description = vcard
            .as_ref()
            .and_then(|card| card.get_child("DESC", NSChoice::Any))
            .map(Element::text)
            .filter(|description| !description.is_empty());
        let subscriptions = self.fetch_subscriptions(&bare).await;
        Ok(ContactProfile {
            jid: bare.to_string(),
            name,
            description,
            subscriptions,
        })
    }
}

fn route_iq(event: ConnectionEvent, router: &IqRouter) {
    let ConnectionEvent::StanzaReceived(xml) = event else {
        return;
    };
    let Ok(element) = xml.parse::<Element>() else {
        return;
    };
    if element.name() != "iq" {
        return;
    }
    let Some(id) = element.attr("id").map(str::to_string) else {
        return;
    };
    let is_result = element.attr("type") == Some("result");
    let is_error = element.attr("type") == Some("error");
    let outcome = if is_result {
        Ok(element)
    } else if is_error {
        Err(xmpp_core::iq::parse_iq_error(&element))
    } else {
        return;
    };
    router.resolve(&id, outcome);
}

fn classify_error(message: &str) -> ContentError {
    let lower = message.to_lowercase();
    if lower.contains("item-not-found")
        || lower.contains("forbidden")
        || lower.contains("not-authorized")
        || lower.contains("not-allowed")
    {
        ContentError::NotFound
    } else {
        ContentError::Unavailable(message.to_string())
    }
}

fn post_from_item(item: PubSubItem) -> Post {
    let html = item
        .html
        .as_deref()
        .and_then(xmpp_core::microblog::sanitize_html);
    Post {
        id: item.id,
        author_jid: item.publisher.to_bare().to_string(),
        author_name: item.author_name,
        title: item.title,
        body: item.body.unwrap_or_default(),
        html,
        published: item.published_at,
        tags: item.tags,
        media: item.media.into_iter().map(media_link).collect(),
    }
}

fn media_link(attachment: MediaAttachment) -> MediaLink {
    MediaLink {
        url: attachment.url,
        mime: attachment.mime,
        name: attachment.name,
    }
}

fn form_value(form: &Element, var: &str) -> Option<String> {
    form.children()
        .find(|field| field.name() == "field" && field.attr("var") == Some(var))
        .and_then(|field| field.get_child("value", NS_DATA_FORMS))
        .map(Element::text)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_public_visibility_errors_as_not_found() {
        assert_eq!(
            classify_error("Protocol error: item-not-found"),
            ContentError::NotFound
        );
        assert_eq!(
            classify_error("Protocol error: forbidden"),
            ContentError::NotFound
        );
        assert!(matches!(
            classify_error("Timeout: no reply"),
            ContentError::Unavailable(_)
        ));
    }

    #[test]
    fn maps_a_pubsub_item_to_a_public_post() {
        let xml = r#"<item xmlns="http://jabber.org/protocol/pubsub" id="p1"><entry xmlns="http://www.w3.org/2005/Atom"><id>p1</id><title>Hello</title><author><uri>xmpp:alice@example.org</uri></author><content>Body</content><published>2026-01-01T00:00:00Z</published><category term="rust"/><link rel="enclosure" href="https://files.example/a.png" type="image/png" title="a.png" length="10"/></entry></item>"#;
        let item = Element::from_reader(xml.as_bytes()).expect("item");
        let service: Jid = "alice@example.org".parse().unwrap();
        let parsed = parse_item_for_node(&item, &service, MICROBLOG_NODE).expect("post");
        let post = post_from_item(parsed);
        assert_eq!(post.id, "p1");
        assert_eq!(post.title.as_deref(), Some("Hello"));
        assert_eq!(post.body, "Body");
        assert_eq!(post.tags, vec!["rust".to_string()]);
        assert_eq!(post.media[0].url, "https://files.example/a.png");
    }
}
