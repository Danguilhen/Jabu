//! XMPP Core - Direct tokio-xmpp implementation for Jabu

pub mod chat;
pub mod connection;
pub mod disco;
pub mod error;
pub mod events;
pub mod microblog;
pub mod muc;
pub mod omemo;
pub mod processor;
pub mod pubsub;
pub mod push;
pub mod roster;
pub mod sm;
pub mod storage;
pub mod types;

pub use chat::{Chat, ChatManager, ChatMessage, ChatState, MessageReceipt, MessageType};
pub use connection::{Connection, ConnectionConfig, ConnectionEvent, ConnectionState};
pub use disco::{DiscoManager, UploadService, UploadSlot};
pub use error::{Result, XmppError};
pub use events::{EventBus, XmppEvent};
pub use microblog::{build_entry, parse_item, MICROBLOG_NODE};
pub use muc::{MucAffiliation, MucConfig, MucManager, MucMessage, MucOccupant, MucRole, MucRoom};
pub use omemo::{
    DeviceBundle, LocalDevice, OmemoEnvelope, OmemoKey, OmemoManager, OmemoStore, PreKeyPair,
    PreKeyPublic, TrustLevel,
};
pub use processor::EventProcessor;
pub use pubsub::{PubSubItem, PubSubManager, PubSubNode, PubSubService, PubSubSubscription};
pub use push::{PushManager, PushRegistration};
pub use roster::{Presence, Roster, RosterGroup, RosterItem, SubscriptionState};
pub use sm::{SmEnabled, StreamManagement};
pub use types::{EncryptionInfo, MediaAttachment, MediaType, Reaction};

use dioxus::prelude::{Readable, Signal, Writable};
use jid::Jid;
use minidom::Element;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use xmpp_parsers::ns;

/// Main XMPP client context that holds all state
#[derive(Clone)]
pub struct XmppClient {
    pub connection: Arc<Connection>,
    pub roster: Arc<Roster>,
    pub chats: Arc<ChatManager>,
    pub mucs: Arc<MucManager>,
    pub pubsub: Arc<PubSubManager>,
    pub disco: Arc<DiscoManager>,
    pub push: Arc<PushManager>,
    pub sm: Arc<StreamManagement>,
    pub omemo: Arc<OmemoManager>,
    pub events: Arc<EventBus>,
}

impl XmppClient {
    /// Send a 1:1 chat message and record it locally.
    pub fn send_message(&self, to: &Jid, body: &str) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();

        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", id.clone())
            .append(Element::builder("body", ns::JABBER_CLIENT).append(body.to_string()))
            .append(Element::builder("request", "urn:xmpp:receipts"))
            .append(Element::builder(
                "active",
                "http://jabber.org/protocol/chatstates",
            ))
            .build();

        self.connection.send_stanza_now(stanza)?;

        let own_jid = self.connection.jid().unwrap_or_else(|| to.clone());
        let mut msg = ChatMessage::new(
            to.to_bare().to_string(),
            own_jid,
            to.clone(),
            true,
            Some(body.to_string()),
        );
        msg.id = id;

        let mut chat = self.chats.get_or_create(to);
        chat.add_message(msg.clone());

        Ok(msg)
    }

    /// Send a group chat message to a MUC room and record it locally.
    pub fn send_muc_message(&self, room: &Jid, nick: &str, body: &str) -> Result<MucMessage> {
        let id = Uuid::new_v4().to_string();

        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "groupchat")
            .attr("to", room.to_string())
            .attr("id", id.clone())
            .append(Element::builder("body", ns::JABBER_CLIENT).append(body.to_string()))
            .build();

        self.connection.send_stanza_now(stanza)?;

        let mut msg = MucMessage::new(
            room.to_bare().to_string(),
            nick.to_string(),
            self.connection.jid(),
            true,
            Some(body.to_string()),
        );
        msg.id = id;

        let mut room_ref = self.mucs.get_or_create(room);
        room_ref.add_message(msg.clone());

        Ok(msg)
    }

    /// Fetch a contact's OMEMO device list and bundles.
    pub fn fetch_omemo(&self, jid: &Jid) -> Result<()> {
        self.fetch_omemo_device_list(jid)?;
        self.fetch_omemo_bundles(jid)
    }

    /// Bundles for the devices currently known for a contact.
    pub fn omemo_bundles(&self, jid: &Jid) -> Vec<DeviceBundle> {
        let bare = jid.to_bare().to_string();
        self.omemo
            .device_list(&bare)
            .into_iter()
            .filter_map(|id| self.omemo.bundle(&bare, id))
            .collect()
    }

    /// Encrypt and send an OMEMO message (XEP-0384) to every known device.
    pub fn send_omemo_message(&self, to: &Jid, body: &str) -> Result<ChatMessage> {
        let bundles = self.omemo_bundles(to);
        if bundles.is_empty() {
            return Err(XmppError::CryptoError(
                "no OMEMO bundles available for recipient".into(),
            ));
        }
        let envelope = self
            .omemo
            .encrypt_message(&to.to_bare().to_string(), body, &bundles)?;
        let id = Uuid::new_v4().to_string();

        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", id.clone())
            .append(envelope.to_element())
            .append(Element::builder("store", "urn:xmpp:hints"))
            .append(
                Element::builder("encryption", "urn:xmpp:eme:0")
                    .attr("namespace", omemo::NS_OMEMO)
                    .attr("name", "OMEMO"),
            )
            .build();
        self.connection.send_stanza_now(stanza)?;

        let own_jid = self.connection.jid().unwrap_or_else(|| to.clone());
        let mut msg = ChatMessage::new(
            to.to_bare().to_string(),
            own_jid,
            to.clone(),
            true,
            Some(body.to_string()),
        );
        msg.id = id;
        msg.encryption = Some(EncryptionInfo {
            namespace: omemo::NS_OMEMO.to_string(),
            name: "OMEMO".to_string(),
            fingerprint: None,
        });

        let mut chat = self.chats.get_or_create(to);
        chat.add_message(msg.clone());

        Ok(msg)
    }

    /// Send a chat state notification (XEP-0085): "composing", "paused", "active", ...
    pub fn send_chat_state(&self, to: &Jid, state: &str) -> Result<()> {
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .append(Element::builder(
                state,
                "http://jabber.org/protocol/chatstates",
            ))
            .build();
        self.connection.send_stanza_now(stanza)
    }

    /// Send a chat marker (XEP-0333): "displayed", "received", "acknowledged".
    pub fn send_marker(&self, to: &Jid, marker: &str, message_id: &str) -> Result<()> {
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .append(
                Element::builder(marker, "urn:xmpp:chat-markers:0")
                    .attr("id", message_id.to_string()),
            )
            .build();
        self.connection.send_stanza_now(stanza)
    }

    /// Send a reply (XEP-0461) to a specific message and record it locally.
    pub fn send_reply(&self, to: &Jid, body: &str, reply_to_id: &str) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", id.clone())
            .append(Element::builder("body", ns::JABBER_CLIENT).append(body.to_string()))
            .append(
                Element::builder("reply", "urn:xmpp:reply:0")
                    .attr("id", reply_to_id.to_string())
                    .attr("to", to.to_bare().to_string()),
            )
            .append(Element::builder("request", "urn:xmpp:receipts"))
            .append(Element::builder(
                "active",
                "http://jabber.org/protocol/chatstates",
            ))
            .build();
        self.connection.send_stanza_now(stanza)?;

        let own = self.connection.jid().unwrap_or_else(|| to.clone());
        let mut msg = ChatMessage::new(
            to.to_bare().to_string(),
            own,
            to.clone(),
            true,
            Some(body.to_string()),
        );
        msg.id = id;
        msg.reply_to = Some(reply_to_id.to_string());

        let mut chat = self.chats.get_or_create(to);
        chat.add_message(msg.clone());
        Ok(msg)
    }

    /// Add reactions to a message (XEP-0444) and update local state.
    pub fn send_reaction(&self, to: &Jid, message_id: &str, emojis: &[&str]) -> Result<()> {
        let mut reactions = Element::builder("reactions", "urn:xmpp:reactions:0")
            .attr("id", message_id.to_string());
        for e in emojis {
            reactions = reactions
                .append(Element::builder("reaction", "urn:xmpp:reactions:0").append(e.to_string()));
        }

        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", format!("react-{}", Uuid::new_v4()))
            .append(reactions)
            .build();
        self.connection.send_stanza_now(stanza)?;

        if let Some(mut chat) = self.chats.get(to) {
            let own = self.connection.jid().unwrap_or_else(|| to.clone());
            let owned: Vec<String> = emojis.iter().map(|s| s.to_string()).collect();
            chat.set_reactions_from(message_id, &own, &owned);
        }
        Ok(())
    }

    /// Correct a previously sent message (XEP-0308) and update local state.
    pub fn send_edit(&self, to: &Jid, message_id: &str, new_body: &str) -> Result<()> {
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", format!("edit-{}", Uuid::new_v4()))
            .append(Element::builder("body", ns::JABBER_CLIENT).append(new_body.to_string()))
            .append(
                Element::builder("replace", "urn:xmpp:message-correct:0")
                    .attr("id", message_id.to_string()),
            )
            .build();
        self.connection.send_stanza_now(stanza)?;

        if let Some(mut chat) = self.chats.get(to) {
            chat.apply_edit(message_id, new_body);
        }
        Ok(())
    }

    /// Retract a previously sent message (XEP-0424) and update local state.
    pub fn send_retract(&self, to: &Jid, message_id: &str) -> Result<()> {
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", format!("retract-{}", Uuid::new_v4()))
            .append(
                Element::builder("retract", "urn:xmpp:message-retract:0")
                    .attr("id", message_id.to_string()),
            )
            .build();
        self.connection.send_stanza_now(stanza)?;

        if let Some(mut chat) = self.chats.get(to) {
            chat.retract(message_id);
        }
        Ok(())
    }

    /// Request a contact's vCard (XEP-0054), which carries their avatar.
    pub fn request_vcard(&self, jid: &Jid) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", jid.to_bare().to_string())
            .attr("id", format!("vcard-{}", Uuid::new_v4()))
            .append(Element::builder("vCard", "vcard-temp"))
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Start service discovery (XEP-0030) on the server domain.
    pub fn discover_server(&self) -> Result<()> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let domain = own.domain().to_string();
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", domain)
            .attr("id", format!("disco-{}", Uuid::new_v4()))
            .append(Element::builder(
                "query",
                "http://jabber.org/protocol/disco#items",
            ))
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Fetch recent entries from a user's microblog node (XEP-0277).
    pub fn fetch_microblog(&self, jid: &Jid, max_items: u32) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", jid.to_bare().to_string())
            .attr("id", format!("feed-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("items", "http://jabber.org/protocol/pubsub")
                        .attr("node", MICROBLOG_NODE)
                        .attr("max_items", max_items.to_string()),
                ),
            )
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Subscribe to a user's microblog node so new posts arrive as events.
    pub fn subscribe_microblog(&self, jid: &Jid) -> Result<()> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", jid.to_bare().to_string())
            .attr("id", format!("feed-sub-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("subscribe", "http://jabber.org/protocol/pubsub")
                        .attr("node", MICROBLOG_NODE)
                        .attr("jid", own.to_bare().to_string()),
                ),
            )
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Publish a post to our own microblog node (XEP-0277).
    pub fn publish_microblog(&self, body: &str, title: Option<&str>) -> Result<PubSubItem> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let own_bare = own.to_bare();
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let entry = microblog::build_entry(&id, body, title, &own_bare.to_string(), now);

        let form = Element::builder("x", "jabber:x:data")
            .attr("type", "submit")
            .append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", "FORM_TYPE")
                    .attr("type", "hidden")
                    .append(
                        Element::builder("value", "jabber:x:data")
                            .append("http://jabber.org/protocol/pubsub#publish-options"),
                    ),
            )
            .append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", "pubsub#access_model")
                    .append(Element::builder("value", "jabber:x:data").append("presence")),
            );

        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", own_bare.to_string())
            .attr("id", format!("post-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub")
                    .append(
                        Element::builder("publish", "http://jabber.org/protocol/pubsub")
                            .attr("node", MICROBLOG_NODE)
                            .append(
                                Element::builder("item", "http://jabber.org/protocol/pubsub")
                                    .attr("id", id.clone())
                                    .append(entry),
                            ),
                    )
                    .append(
                        Element::builder("publish-options", "http://jabber.org/protocol/pubsub")
                            .append(form),
                    ),
            )
            .build();
        self.connection.send_stanza_now(iq)?;

        let mut item = PubSubItem::from_microblog(
            MICROBLOG_NODE.to_string(),
            Jid::from(own_bare.clone()),
            body.to_string(),
            Vec::new(),
            Vec::new(),
        );
        item.id = id;
        item.title = title.map(|t| t.to_string());
        item.published_at = now;

        let service = self.pubsub.get_or_create(&Jid::from(own_bare));
        let mut node = service.get_or_create_node(MICROBLOG_NODE.to_string());
        node.add_item(item.clone());

        Ok(item)
    }

    /// Publish this device's OMEMO device list to our PEP node.
    pub fn publish_omemo_device_list(&self, device_ids: &[u32]) -> Result<()> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let mut devices = Element::builder("devices", omemo::NS_OMEMO);
        for id in device_ids {
            devices = devices
                .append(Element::builder("device", omemo::NS_OMEMO).attr("id", id.to_string()));
        }
        self.publish_pep(
            &own.to_bare(),
            omemo::NS_DEVICES,
            Some("current"),
            devices.build(),
        )
    }

    /// Publish this device's bundle to our PEP bundle node.
    pub fn publish_omemo_bundle(&self, bundle: &DeviceBundle) -> Result<()> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        self.publish_pep(
            &own.to_bare(),
            omemo::NS_BUNDLES,
            Some(&bundle.device_id.to_string()),
            bundle.to_element(),
        )
    }

    /// Fetch a contact's OMEMO device list.
    pub fn fetch_omemo_device_list(&self, jid: &Jid) -> Result<()> {
        self.fetch_pep_items(jid, omemo::NS_DEVICES, 1)
    }

    /// Fetch a contact's OMEMO bundles (one item per device).
    pub fn fetch_omemo_bundles(&self, jid: &Jid) -> Result<()> {
        self.fetch_pep_items(jid, omemo::NS_BUNDLES, 100)
    }

    fn publish_pep(
        &self,
        to: &Jid,
        node: &str,
        item_id: Option<&str>,
        payload: Element,
    ) -> Result<()> {
        let mut item = Element::builder("item", "http://jabber.org/protocol/pubsub");
        if let Some(id) = item_id {
            item = item.attr("id", id.to_string());
        }
        let item = item.append(payload).build();

        let form = Element::builder("x", "jabber:x:data")
            .attr("type", "submit")
            .append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", "FORM_TYPE")
                    .attr("type", "hidden")
                    .append(
                        Element::builder("value", "jabber:x:data")
                            .append("http://jabber.org/protocol/pubsub#publish-options"),
                    ),
            )
            .append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", "pubsub#access_model")
                    .append(Element::builder("value", "jabber:x:data").append("open")),
            );

        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", to.to_bare().to_string())
            .attr("id", format!("pep-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub")
                    .append(
                        Element::builder("publish", "http://jabber.org/protocol/pubsub")
                            .attr("node", node)
                            .append(item),
                    )
                    .append(
                        Element::builder("publish-options", "http://jabber.org/protocol/pubsub")
                            .append(form),
                    ),
            )
            .build();
        self.connection.send_stanza_now(iq)
    }

    fn fetch_pep_items(&self, jid: &Jid, node: &str, max_items: u32) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", jid.to_bare().to_string())
            .attr("id", format!("pep-get-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("items", "http://jabber.org/protocol/pubsub")
                        .attr("node", node)
                        .attr("max_items", max_items.to_string()),
                ),
            )
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Register a push notification service (XEP-0357).
    pub fn enable_push(&self, service: &Jid, node: &str, secret: Option<&str>) -> Result<()> {
        let mut enable = Element::builder("enable", "urn:xmpp:push:0")
            .attr("jid", service.to_bare().to_string())
            .attr("node", node.to_string());
        if let Some(secret) = secret {
            enable = enable.append(
                Element::builder("x", "jabber:x:data")
                    .attr("type", "submit")
                    .append(
                        Element::builder("field", "jabber:x:data")
                            .attr("var", "FORM_TYPE")
                            .attr("type", "hidden")
                            .append(
                                Element::builder("value", "jabber:x:data")
                                    .append("http://jabber.org/protocol/pubsub#publish-options"),
                            ),
                    )
                    .append(
                        Element::builder("field", "jabber:x:data")
                            .attr("var", "secret")
                            .append(
                                Element::builder("value", "jabber:x:data")
                                    .append(secret.to_string()),
                            ),
                    )
                    .build(),
            );
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("id", format!("push-enable-{}", Uuid::new_v4()))
            .append(enable.build())
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Register push by JID string (convenience wrapper).
    pub fn enable_push_str(&self, service: &str, node: &str, secret: Option<&str>) -> Result<()> {
        let jid: Jid = service
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.enable_push(&jid, node, secret)
    }

    /// Unregister a push notification service (XEP-0357).
    pub fn disable_push(&self, service: &Jid, node: &str) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("id", format!("push-disable-{}", Uuid::new_v4()))
            .append(
                Element::builder("disable", "urn:xmpp:push:0")
                    .attr("jid", service.to_bare().to_string())
                    .attr("node", node.to_string()),
            )
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Unregister push by JID string (convenience wrapper).
    pub fn disable_push_str(&self, service: &str, node: &str) -> Result<()> {
        let jid: Jid = service
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.disable_push(&jid, node)
    }

    /// Ask the upload service for a slot (XEP-0363).
    pub fn request_upload_slot(
        &self,
        id: &str,
        filename: &str,
        size: u64,
        content_type: &str,
    ) -> Result<()> {
        let service = self
            .disco
            .upload_service()
            .ok_or_else(|| XmppError::FeatureNotSupported("HTTP file upload".into()))?;
        let request = Element::builder("request", "urn:xmpp:http:upload:0")
            .attr("filename", filename)
            .attr("size", size.to_string())
            .attr("content-type", content_type)
            .build();
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", service.jid)
            .attr("id", id)
            .append(request)
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Ask the upload service for a slot and await its reply.
    async fn obtain_upload_slot(
        &self,
        filename: &str,
        size: u64,
        content_type: &str,
    ) -> Result<UploadSlot> {
        let service = self
            .disco
            .upload_service()
            .ok_or_else(|| XmppError::FeatureNotSupported("HTTP file upload".into()))?;
        if let Some(max) = service.max_file_size {
            if size > max {
                return Err(XmppError::UploadError(format!(
                    "file is {} bytes, service allows at most {}",
                    size, max
                )));
            }
        }

        let id = format!("upload-{}", Uuid::new_v4());
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.disco.register_slot(id.clone(), tx);
        if let Err(e) = self.request_upload_slot(&id, filename, size, content_type) {
            self.disco.cancel_slot(&id);
            return Err(e);
        }

        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(slot)) => Ok(slot),
            Ok(Err(_)) => Err(XmppError::UploadError(
                "upload service closed the request".into(),
            )),
            Err(_) => {
                self.disco.cancel_slot(&id);
                Err(XmppError::Timeout("upload slot request".into()))
            }
        }
    }

    /// Upload bytes to the discovered XEP-0363 service and return the slot
    /// (whose `get_url` is the public URL to share).
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn upload_file(
        &self,
        filename: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<UploadSlot> {
        let slot = self
            .obtain_upload_slot(filename, bytes.len() as u64, content_type)
            .await?;

        let mut req = reqwest::Client::new().put(&slot.put_url).body(bytes);
        for (name, value) in &slot.headers {
            req = req.header(name.as_str(), value.as_str());
        }
        let resp = req
            .send()
            .await
            .map_err(|e| XmppError::UploadError(format!("PUT failed: {}", e)))?;
        if !resp.status().is_success() {
            return Err(XmppError::UploadError(format!(
                "PUT returned HTTP {}",
                resp.status()
            )));
        }

        Ok(slot)
    }

    /// Upload bytes to the discovered XEP-0363 service and return the slot.
    #[cfg(target_arch = "wasm32")]
    pub async fn upload_file(
        &self,
        filename: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<UploadSlot> {
        let slot = self
            .obtain_upload_slot(filename, bytes.len() as u64, content_type)
            .await?;

        let mut req = gloo_net::http::Request::put(&slot.put_url);
        for (name, value) in &slot.headers {
            req = req.header(name.as_str(), value.as_str());
        }
        let req = req
            .body(bytes)
            .map_err(|e| XmppError::UploadError(format!("PUT failed: {}", e)))?;
        let resp = req
            .send()
            .await
            .map_err(|e| XmppError::UploadError(format!("PUT failed: {}", e)))?;
        if !resp.ok() {
            return Err(XmppError::UploadError(format!(
                "PUT returned HTTP {}",
                resp.status()
            )));
        }

        Ok(slot)
    }

    /// Change a room subject (XEP-0045).
    pub fn send_muc_subject(&self, room: &Jid, subject: &str) -> Result<()> {
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "groupchat")
            .attr("to", room.to_bare().to_string())
            .append(Element::builder("subject", ns::JABBER_CLIENT).append(subject.to_string()))
            .build();
        self.connection.send_stanza_now(stanza)?;

        let mut room_ref = self.mucs.get_or_create(room);
        room_ref.set_subject(subject.to_string(), self.connection.jid());
        Ok(())
    }

    /// Change a participant's role (XEP-0045 moderation): e.g. `none` to kick,
    /// `visitor`, `participant`, `moderator`.
    pub fn set_muc_role(
        &self,
        room: &Jid,
        nick: &str,
        role: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        let mut item = Element::builder("item", "http://jabber.org/protocol/muc#admin")
            .attr("nick", nick.to_string())
            .attr("role", role.to_string());
        if let Some(reason) = reason {
            item = item.append(
                Element::builder("reason", "http://jabber.org/protocol/muc#admin")
                    .append(reason.to_string()),
            );
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", room.to_bare().to_string())
            .attr("id", format!("muc-role-{}", Uuid::new_v4()))
            .append(
                Element::builder("query", "http://jabber.org/protocol/muc#admin")
                    .append(item.build()),
            )
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Change a participant's affiliation (XEP-0045): `outcast` to ban,
    /// `member`, `admin`, `owner`, or `none` to restore.
    pub fn set_muc_affiliation(
        &self,
        room: &Jid,
        jid: &Jid,
        affiliation: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        let mut item = Element::builder("item", "http://jabber.org/protocol/muc#admin")
            .attr("jid", jid.to_bare().to_string())
            .attr("affiliation", affiliation.to_string());
        if let Some(reason) = reason {
            item = item.append(
                Element::builder("reason", "http://jabber.org/protocol/muc#admin")
                    .append(reason.to_string()),
            );
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", room.to_bare().to_string())
            .attr("id", format!("muc-aff-{}", Uuid::new_v4()))
            .append(
                Element::builder("query", "http://jabber.org/protocol/muc#admin")
                    .append(item.build()),
            )
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Invite someone to a room (mediated, XEP-0045).
    pub fn invite_to_muc(&self, room: &Jid, invitee: &Jid, reason: Option<&str>) -> Result<()> {
        let mut invite = Element::builder("invite", "http://jabber.org/protocol/muc#user")
            .attr("to", invitee.to_bare().to_string());
        if let Some(reason) = reason {
            invite = invite.append(
                Element::builder("reason", "http://jabber.org/protocol/muc#user")
                    .append(reason.to_string()),
            );
        }
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("to", room.to_bare().to_string())
            .append(
                Element::builder("x", "http://jabber.org/protocol/muc#user").append(invite.build()),
            )
            .build();
        self.connection.send_stanza_now(stanza)
    }

    /// Request a contact's vCard by bare or full JID string.
    pub fn request_vcard_str(&self, jid: &str) -> Result<()> {
        let jid = jid
            .parse::<Jid>()
            .map_err(|e| XmppError::InvalidJid(e.to_string()))?;
        self.request_vcard(&jid)
    }

    /// Request our own vCard (no `to`).
    pub fn request_own_vcard(&self) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("id", format!("vcard-own-{}", Uuid::new_v4()))
            .append(Element::builder("vCard", "vcard-temp"))
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Join a MUC room (XEP-0045) by sending presence to `room@service/nick`.
    pub fn join_room(&self, room: &Jid, nick: &str, password: Option<&str>) -> Result<MucRoom> {
        let mut muc = Element::builder("x", "http://jabber.org/protocol/muc");
        if let Some(pw) = password {
            muc = muc.append(
                Element::builder("password", "http://jabber.org/protocol/muc")
                    .append(pw.to_string()),
            );
        }
        let presence = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("to", format!("{}/{}", room.to_bare(), nick))
            .append(muc)
            .build();
        self.connection.send_stanza_now(presence)?;

        let mut room_ref = self.mucs.get_or_create(room);
        room_ref.join(nick, password.map(|p| p.to_string()))?;
        Ok(room_ref)
    }

    /// Create an instant room and join it with the given nick.
    pub fn create_room(&self, room: &Jid, nick: &str) -> Result<MucRoom> {
        let presence = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("to", format!("{}/{}", room.to_bare(), nick))
            .append(Element::builder("x", "http://jabber.org/protocol/muc"))
            .build();
        self.connection.send_stanza_now(presence)?;

        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", room.to_bare().to_string())
            .attr("id", format!("create-{}", Uuid::new_v4()))
            .append(
                Element::builder("query", "http://jabber.org/protocol/muc#owner")
                    .append(Element::builder("x", "jabber:x:data").attr("type", "submit")),
            )
            .build();
        self.connection.send_stanza_now(iq)?;

        let mut room_ref = self.mucs.get_or_create(room);
        room_ref.join(nick, None)?;
        Ok(room_ref)
    }

    /// Create a room given as a string (convenience wrapper).
    pub fn create_room_str(&self, room: &str, nick: &str) -> Result<MucRoom> {
        let jid: Jid = room
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.create_room(&jid, nick)
    }

    /// Leave a MUC room.
    pub fn leave_room(&self, room: &Jid) -> Result<()> {
        let presence = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("to", format!("{}/{}", room.to_bare(), "me"))
            .attr("type", "unavailable")
            .build();
        self.connection.send_stanza_now(presence)?;
        if let Some(mut r) = self.mucs.get_by_bare(&room.to_bare().to_string()) {
            r.leave(None)?;
        }
        Ok(())
    }

    /// Add a contact to the roster and request presence subscription.
    pub fn add_contact(&self, jid: &Jid, name: Option<&str>) -> Result<()> {
        let mut item =
            Element::builder("item", "jabber:iq:roster").attr("jid", jid.to_bare().to_string());
        if let Some(n) = name {
            item = item.attr("name", n.to_string());
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("id", format!("roster-set-{}", Uuid::new_v4()))
            .append(Element::builder("query", "jabber:iq:roster").append(item))
            .build();
        self.connection.send_stanza_now(iq)?;

        let subscribe = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("type", "subscribe")
            .attr("to", jid.to_bare().to_string())
            .build();
        self.connection.send_stanza_now(subscribe)?;
        Ok(())
    }

    /// Add a contact by its string JID (convenience wrapper for the UI).
    pub fn add_contact_str(&self, jid: &str, name: Option<&str>) -> Result<()> {
        let jid: Jid = jid
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.add_contact(&jid, name)
    }

    /// Join a room by its string JID (convenience wrapper for the UI).
    pub fn join_room_str(&self, room: &str, nick: &str) -> Result<MucRoom> {
        let jid: Jid = room
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.join_room(&jid, nick, None)
    }

    /// Query the archive for a conversation given as string (convenience wrapper).
    pub fn query_mam_str(&self, with: &str, max: u32) -> Result<()> {
        let jid: Jid = with
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.query_mam(&jid, max)
    }

    /// Query the message archive (XEP-0313, `urn:xmpp:mam:2`) for a conversation.
    pub fn query_mam(&self, with: &Jid, max: u32) -> Result<()> {
        let query_id = format!("q-{}", Uuid::new_v4());
        let fields = Element::builder("x", "jabber:x:data")
            .attr("type", "submit")
            .append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", "FORM_TYPE")
                    .attr("type", "hidden")
                    .append(Element::builder("value", "jabber:x:data").append("urn:xmpp:mam:2")),
            )
            .append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", "with")
                    .append(
                        Element::builder("value", "jabber:x:data")
                            .append(with.to_bare().to_string()),
                    ),
            );

        let query = Element::builder("query", "urn:xmpp:mam:2")
            .attr("queryid", query_id)
            .append(fields)
            .append(
                Element::builder("set", "http://jabber.org/protocol/rsm").append(
                    Element::builder("max", "http://jabber.org/protocol/rsm")
                        .append(max.to_string()),
                ),
            );

        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("id", format!("mam-{}", Uuid::new_v4()))
            .append(query)
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Query a room's message archive (XEP-0313) by room JID string.
    pub fn query_muc_mam_str(&self, room: &str, max: u32) -> Result<()> {
        let jid: Jid = room
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.query_muc_mam(&jid, max)
    }

    /// Query the message archive of a MUC room.
    pub fn query_muc_mam(&self, room: &Jid, max: u32) -> Result<()> {
        let query_id = format!("muc-q-{}", Uuid::new_v4());
        let fields = Element::builder("x", "jabber:x:data")
            .attr("type", "submit")
            .append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", "FORM_TYPE")
                    .attr("type", "hidden")
                    .append(Element::builder("value", "jabber:x:data").append("urn:xmpp:mam:2")),
            );
        let query = Element::builder("query", "urn:xmpp:mam:2")
            .attr("queryid", query_id)
            .append(fields)
            .append(
                Element::builder("set", "http://jabber.org/protocol/rsm").append(
                    Element::builder("max", "http://jabber.org/protocol/rsm")
                        .append(max.to_string()),
                ),
            );
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", room.to_bare().to_string())
            .attr("id", format!("mam-muc-{}", Uuid::new_v4()))
            .append(query)
            .build();
        self.connection.send_stanza_now(iq)
    }
}

/// Configuration for the XMPP client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub jid: String,
    pub password: Option<String>,
    pub resource: Option<String>,
    pub priority: i32,
    pub auto_reconnect: bool,
    pub reconnect_interval: u64,
    pub max_reconnect_attempts: u32,
    pub enable_stream_management: bool,
    pub enable_compression: bool,
    pub keepalive_interval: u64,
    pub tls_config: TlsConfig,
    pub proxy_config: Option<ProxyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub verify_hostname: bool,
    pub verify_cert: bool,
    pub ca_file: Option<String>,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    Socks5,
    Http,
    Https,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            jid: String::new(),
            password: None,
            resource: Some("jabu".to_string()),
            priority: 0,
            auto_reconnect: true,
            reconnect_interval: 5,
            max_reconnect_attempts: 10,
            enable_stream_management: true,
            enable_compression: false,
            keepalive_interval: 60,
            tls_config: TlsConfig::default(),
            proxy_config: None,
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            verify_hostname: true,
            verify_cert: true,
            ca_file: None,
            cert_file: None,
            key_file: None,
        }
    }
}

/// Global client state for Dioxus signals
#[derive(Clone)]
pub struct ClientState {
    pub config: Signal<Option<ClientConfig>>,
    pub client: Signal<Option<XmppClient>>,
    pub connection_state: Signal<ConnectionState>,
    pub is_connected: Signal<bool>,
    pub is_authenticated: Signal<bool>,
    pub last_error: Signal<Option<String>>,
    /// Bumped whenever the underlying XMPP data changes, so components re-read it.
    pub revision: Signal<u64>,
    /// Bare JID of the currently open conversation (1:1 chat or MUC room).
    pub selected_chat: Signal<Option<String>>,
    /// Whether the main area shows the social feed instead of a conversation.
    pub show_feed: Signal<bool>,
}

impl ClientState {
    pub fn new() -> Self {
        let config = Signal::new(None);
        let client = Signal::new(None);
        let connection_state = Signal::new(ConnectionState::Disconnected);
        let is_connected = Signal::new(false);
        let is_authenticated = Signal::new(false);
        let last_error = Signal::new(None);
        let revision = Signal::new(0);
        let selected_chat = Signal::new(None);
        let show_feed = Signal::new(false);

        Self {
            config,
            client,
            connection_state,
            is_connected,
            is_authenticated,
            last_error,
            revision,
            selected_chat,
            show_feed,
        }
    }

    /// Signal the UI that underlying data changed.
    pub fn bump(&mut self) {
        let next = *self.revision.read() + 1;
        self.revision.set(next);
    }

    /// Select a conversation by bare JID (or clear it).
    pub fn select(&mut self, jid: Option<String>) {
        self.selected_chat.set(jid);
    }
}

impl Default for ClientState {
    fn default() -> Self {
        Self::new()
    }
}

impl From<ClientConfig> for ConnectionConfig {
    fn from(config: ClientConfig) -> Self {
        Self {
            jid: config.jid,
            password: config.password,
            resource: config.resource,
            priority: config.priority,
            auto_reconnect: config.auto_reconnect,
            reconnect_interval: config.reconnect_interval,
            max_reconnect_attempts: config.max_reconnect_attempts,
            enable_stream_management: config.enable_stream_management,
            enable_compression: config.enable_compression,
            keepalive_interval: config.keepalive_interval,
            tls_config: config.tls_config,
            proxy_config: config.proxy_config,
        }
    }
}
