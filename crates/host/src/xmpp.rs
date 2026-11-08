//! XMPP-backed public content source.
//!
//! The Host is a headless XMPP client: it connects with its own account and
//! fetches public `PubSub` nodes on demand. Only IQ results are routed; chat,
//! presence and encryption managers are not needed for public pages.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use jid::Jid;
use minidom::{Element, NSChoice};
use xmpp_core::{
    microblog::{parse_item_for_node, MICROBLOG_NODE},
    Connection, ConnectionConfig, ConnectionEvent, IqRouter, MediaAttachment, PubSubItem,
};

use crate::content::{BlogProfile, ContentError, ContentSource, MediaLink, Post};

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

    async fn fetch_posts(
        &self,
        jid: &Jid,
        node: &str,
        max: u32,
    ) -> Result<Vec<Post>, ContentError> {
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
            .filter_map(|item| parse_item_for_node(item, jid, node))
            .map(post_from_item)
            .collect())
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
    Post {
        id: item.id,
        author_jid: item.publisher.to_bare().to_string(),
        title: item.title,
        body: item.body.or_else(|| item.html.clone()).unwrap_or_default(),
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
