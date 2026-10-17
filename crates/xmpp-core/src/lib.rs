//! XMPP Core - Direct tokio-xmpp implementation for Jabu

pub mod adhoc;
pub mod avatar;
pub mod bob;
pub mod caps;
pub mod change;
pub mod chat;
pub mod color;
pub mod connection;
pub mod data_form;
pub mod disco;
pub mod error;
pub mod events;
pub mod gateway;
pub mod iq;
pub mod jingle;
pub mod mam;
pub mod microblog;
pub mod muc;
pub mod omemo;
pub mod pep;
pub mod presence;
pub mod processor;
pub mod pubsub;
pub mod push;
pub mod roster;
pub mod sm;
pub mod types;
pub mod vcard;
pub mod vcard4;

pub use adhoc::{AdHocCommand, AdHocResponse};
pub use bob::{BobContent, BobRequest};
pub use caps::{caps_element, verification as caps_verification};
pub use chat::{
    Chat, ChatManager, ChatMessage, ChatPreferences, ChatState, MessageReceipt, MessageType,
};
pub use color::identity_color;
pub use connection::{Connection, ConnectionConfig, ConnectionEvent, ConnectionState};
pub use data_form::{DataForm, FormField};
pub use disco::{
    parse_reaction_restrictions, DiscoManager, ExternalService, ReactionRestrictions, ServerInfo,
    UploadService, UploadSlot,
};
pub use error::{Result, XmppError};
pub use events::{EventBus, XmppEvent};
pub use gateway::{Gateway, GatewayField, GatewayRegistration};
pub use iq::{IqError, IqRouter};
pub use jingle::{CallDirection, CallKind, CallSession, CallState, JingleManager, MediaPort};
pub use mam::{MamManager, MamQuery};
pub use microblog::{build_entry, parse_item, MICROBLOG_NODE};
pub use muc::{MucAffiliation, MucConfig, MucManager, MucMessage, MucOccupant, MucRole, MucRoom};
pub use omemo::{
    DeviceBundle, LocalDevice, OmemoEnvelope, OmemoKey, OmemoManager, OmemoStore, PreKeyPair,
    PreKeyPublic, TrustLevel,
};
pub use pep::{activity, location, mood, tune};
pub use presence::presence_element;
pub use processor::{EventProcessor, ProcessorNotifier};
pub use pubsub::{PubSubItem, PubSubManager, PubSubNode, PubSubService, PubSubSubscription};
pub use push::{PushManager, PushRegistration};
pub use roster::{CapsInfo, Presence, Roster, RosterGroup, RosterItem, SubscriptionState};
pub use sm::{SmEnabled, StreamManagement};
pub use types::{EncryptionInfo, MediaAttachment, MediaType, Reaction};
pub use vcard::merge_vcard;
pub use vcard4::VCard4;

pub use jid::Jid;
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
    pub iq: Arc<IqRouter>,
    pub mam: Arc<MamManager>,
    pub jingle: Arc<JingleManager>,
}

fn mention_references(body: &str, room: &Jid, nicks: &[String]) -> Vec<(String, usize, usize)> {
    let chars: Vec<char> = body.chars().collect();
    let mut references: Vec<(String, usize, usize)> = Vec::new();
    for nick in nicks {
        if nick.is_empty() {
            continue;
        }
        let needle: Vec<char> = format!("@{nick}").chars().collect();
        if needle.is_empty() || needle.len() > chars.len() {
            continue;
        }
        for start in 0..=chars.len() - needle.len() {
            if chars[start..start + needle.len()] != needle[..] {
                continue;
            }
            let before_ok = start == 0 || !chars[start - 1].is_alphanumeric();
            let end = start + needle.len();
            let after_ok = end == chars.len() || !chars[end].is_alphanumeric();
            if !before_ok || !after_ok {
                continue;
            }
            let uri = format!("xmpp:{}/{}", room.to_bare(), nick);
            if !references.iter().any(|(existing, _, _)| existing == &uri) {
                references.push((uri, start, end));
            }
        }
    }
    references.sort_by_key(|(_, start, _)| *start);
    references
}

fn forward_stanza(kind: &str, to: &str, id: &str, original: &ChatMessage) -> Element {
    let body = original
        .body
        .clone()
        .filter(|body| !body.is_empty())
        .unwrap_or_else(|| original.display_body());
    let inner_type = match original.message_type {
        MessageType::GroupChat => "groupchat",
        _ => "chat",
    };
    let mut inner = Element::builder("message", ns::JABBER_CLIENT)
        .attr("from", original.from_jid.to_string())
        .attr("type", inner_type)
        .append(Element::builder("body", ns::JABBER_CLIENT).append(body.clone()));
    if let Some(id) = original.origin_id.as_ref() {
        inner =
            inner.append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id.clone()));
    }
    if let Some(attachment) = original.media.first() {
        inner = inner.append(
            Element::builder("x", "jabber:x:oob")
                .append(Element::builder("url", "jabber:x:oob").append(attachment.url.clone()))
                .append(
                    Element::builder("desc", "jabber:x:oob").append(
                        attachment
                            .name
                            .clone()
                            .unwrap_or_else(|| attachment.url.clone()),
                    ),
                ),
        );
    }
    Element::builder("message", ns::JABBER_CLIENT)
        .attr("type", kind)
        .attr("to", to)
        .attr("id", id)
        .append(Element::builder("body", ns::JABBER_CLIENT).append(body))
        .append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id))
        .append(Element::builder("store", "urn:xmpp:hints"))
        .append(
            Element::builder("forwarded", "urn:xmpp:forward:0")
                .append(
                    Element::builder("delay", "urn:xmpp:delay")
                        .attr("stamp", original.timestamp.to_rfc3339()),
                )
                .append(inner),
        )
        .build()
}

fn file_stanza(
    kind: &str,
    to: &str,
    id: &str,
    url: &str,
    name: &str,
    mime: &str,
    size: u64,
) -> Element {
    let mut file = Element::builder("file", "urn:xmpp:file:metadata:0")
        .append(Element::builder("name", "urn:xmpp:file:metadata:0").append(name.to_string()))
        .append(
            Element::builder("media-type", "urn:xmpp:file:metadata:0").append(mime.to_string()),
        );
    if size > 0 {
        file = file
            .append(Element::builder("size", "urn:xmpp:file:metadata:0").append(size.to_string()));
    }
    Element::builder("message", ns::JABBER_CLIENT)
        .attr("type", kind)
        .attr("to", to)
        .attr("id", id)
        .append(Element::builder("body", ns::JABBER_CLIENT).append(url.to_string()))
        .append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id))
        .append(Element::builder("store", "urn:xmpp:hints"))
        .append(
            Element::builder("x", "jabber:x:oob")
                .append(Element::builder("url", "jabber:x:oob").append(url.to_string()))
                .append(Element::builder("desc", "jabber:x:oob").append(name.to_string())),
        )
        .append(file)
        .build()
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
            .append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id.clone()))
            .append(Element::builder("request", "urn:xmpp:receipts"))
            .append(Element::builder("markable", "urn:xmpp:chat-markers:0"))
            .append(Element::builder("store", "urn:xmpp:hints"))
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
        msg.id = id.clone();
        msg.origin_id = Some(id);
        msg.hints.store = true;
        msg.hints.markable = true;
        msg.receipt = crate::chat::MessageReceipt::Sent;

        let mut chat = self.chats.get_or_create(to);
        chat.add_message(msg.clone());

        Ok(msg)
    }

    /// Send an attention request (XEP-0224) and record a local placeholder.
    pub fn send_attention(&self, to: &Jid) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", id.clone())
            .append(Element::builder("attention", "urn:xmpp:attention:0"))
            .append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id.clone()))
            .append(Element::builder("store", "urn:xmpp:hints"))
            .build();
        self.connection.send_stanza_now(stanza)?;

        let own = self.connection.jid().unwrap_or_else(|| to.clone());
        let mut msg = ChatMessage::new(
            to.to_bare().to_string(),
            own,
            to.clone(),
            true,
            Some("Attention".to_string()),
        );
        msg.id = id.clone();
        msg.origin_id = Some(id);
        msg.attention = true;
        msg.hints.store = true;
        let mut chat = self.chats.get_or_create(to);
        chat.add_message(msg.clone());
        Ok(msg)
    }

    /// Install the media backend used by the Jingle control plane.
    pub fn set_call_media(&self, media: Arc<dyn crate::jingle::MediaPort>) {
        self.jingle.set_media(media);
    }

    /// Start forwarding media events into call state (needs an async runtime).
    pub fn start_call_media_listener(&self) {
        self.jingle.start_media_listener();
    }

    /// Start an outgoing audio/video call (XEP-0353 propose).
    pub async fn start_call(&self, to: &Jid, kind: crate::jingle::CallKind) -> Result<String> {
        self.jingle.start_call(to, kind).await
    }

    /// Accept an incoming call.
    pub async fn accept_call(&self, sid: &str) -> Result<()> {
        self.jingle.accept_call(sid).await
    }

    /// Reject an incoming call or cancel an outgoing one.
    pub async fn reject_call(&self, sid: &str, reason: Option<&str>) -> Result<()> {
        self.jingle.reject_call(sid, reason).await
    }

    /// Hang up an established call.
    pub async fn end_call(&self, sid: &str, reason: Option<&str>) -> Result<()> {
        self.jingle.end_call(sid, reason).await
    }

    /// Mute or unmute the local microphone.
    pub async fn set_call_muted(&self, sid: &str, muted: bool) -> Result<()> {
        self.jingle.set_muted(sid, muted).await
    }

    /// All known call sessions, newest first.
    #[must_use]
    pub fn call_sessions(&self) -> Vec<crate::jingle::CallSession> {
        self.jingle.sessions()
    }

    /// One call session by id.
    #[must_use]
    pub fn call_session(&self, sid: &str) -> Option<crate::jingle::CallSession> {
        self.jingle.session(sid)
    }

    /// List the ad-hoc commands an entity advertises (XEP-0050).
    ///
    /// # Errors
    ///
    /// Returns an error when the IQ request fails or times out.
    pub async fn list_commands(&self, jid: &Jid) -> Result<Vec<AdHocCommand>> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", jid.to_bare().to_string())
            .attr("id", format!("adhoc-list-{}", Uuid::new_v4()))
            .append(
                Element::builder("query", crate::adhoc::NS_DISCO_ITEMS)
                    .attr("node", crate::adhoc::NS_COMMANDS),
            )
            .build();
        let reply = self.iq_request(iq).await?;
        let items = reply
            .get_child("query", crate::adhoc::NS_DISCO_ITEMS)
            .ok_or_else(|| XmppError::ProtocolError("no command list returned".into()))?;
        Ok(crate::adhoc::parse_command_list(
            &jid.to_bare().to_string(),
            items,
        ))
    }

    /// Execute an ad-hoc command, optionally with a submitted form (XEP-0050).
    ///
    /// # Errors
    ///
    /// Returns an error when the command is rejected or the reply is malformed.
    pub async fn execute_command(
        &self,
        jid: &Jid,
        node: &str,
        form: Option<Element>,
    ) -> Result<AdHocResponse> {
        let mut command = Element::builder("command", crate::adhoc::NS_COMMANDS)
            .attr("node", node)
            .attr("action", "execute");
        if let Some(form) = form {
            command = command.append(form);
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", jid.to_bare().to_string())
            .attr("id", format!("adhoc-exec-{}", Uuid::new_v4()))
            .append(command.build())
            .build();
        let reply = self.iq_request(iq).await?;
        let command = reply
            .get_child("command", crate::adhoc::NS_COMMANDS)
            .ok_or_else(|| XmppError::ProtocolError("no command reply returned".into()))?;
        Ok(crate::adhoc::parse_response(command))
    }

    /// Discover the gateways the server advertises (XEP-0100).
    ///
    /// # Errors
    ///
    /// Returns an error when the server discovery request fails.
    pub async fn list_gateways(&self) -> Result<Vec<Gateway>> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", own.domain().to_string())
            .attr("id", format!("gateways-{}", Uuid::new_v4()))
            .append(Element::builder("query", crate::gateway::NS_DISCO_ITEMS))
            .build();
        let reply = self.iq_request(iq).await?;
        let items = reply
            .get_child("query", crate::gateway::NS_DISCO_ITEMS)
            .ok_or_else(|| XmppError::ProtocolError("no gateway list returned".into()))?;

        let mut gateways = Vec::new();
        for item in items.children().filter(|child| child.name() == "item") {
            let Some(jid) = item.attr("jid") else {
                continue;
            };
            let Ok(jid) = jid.parse::<Jid>() else {
                continue;
            };
            let info = Element::builder("iq", ns::JABBER_CLIENT)
                .attr("type", "get")
                .attr("to", jid.to_bare().to_string())
                .attr("id", format!("gateway-info-{}", Uuid::new_v4()))
                .append(Element::builder("query", crate::gateway::NS_DISCO_INFO))
                .build();
            let Ok(info) = self.iq_request(info).await else {
                continue;
            };
            let Some(query) = info.get_child("query", crate::gateway::NS_DISCO_INFO) else {
                continue;
            };
            if let Some(gateway) =
                crate::gateway::parse_gateway(&jid.to_bare().to_string(), query, item.attr("name"))
            {
                gateways.push(gateway);
            }
        }
        Ok(gateways)
    }

    /// Fetch a gateway's registration form (XEP-0100).
    ///
    /// # Errors
    ///
    /// Returns an error when the gateway rejects the request.
    pub async fn gateway_registration(&self, gateway: &Jid) -> Result<GatewayRegistration> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", gateway.to_bare().to_string())
            .attr("id", format!("gateway-form-{}", Uuid::new_v4()))
            .append(Element::builder("query", crate::gateway::NS_REGISTER))
            .build();
        let reply = self.iq_request(iq).await?;
        let query = reply
            .get_child("query", crate::gateway::NS_REGISTER)
            .ok_or_else(|| XmppError::ProtocolError("no registration form returned".into()))?;
        Ok(crate::gateway::parse_registration(query))
    }

    /// Submit a gateway registration or remove it (XEP-0100).
    ///
    /// # Errors
    ///
    /// Returns an error when the gateway rejects the submission.
    pub async fn submit_gateway_registration(
        &self,
        gateway: &Jid,
        form: Option<&Element>,
        values: &[(String, String)],
        remove: bool,
    ) -> Result<()> {
        let payload = crate::gateway::registration_payload(form, values, remove);
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", gateway.to_bare().to_string())
            .attr("id", format!("gateway-set-{}", Uuid::new_v4()))
            .append(payload)
            .build();
        self.iq_request(iq).await?;
        Ok(())
    }

    /// Send an IQ and wait for its typed reply, with the router's timeout.
    pub async fn iq_request(&self, iq: Element) -> Result<Element> {
        self.iq
            .request(|stanza| self.connection.send_stanza_now(stanza.clone()), iq)
            .await
    }

    /// Tell the server whether this client is foreground (XEP-0352).
    ///
    /// The preference is kept even before the server has advertised support, so
    /// it is applied as soon as service discovery reports the feature.
    pub fn set_csi_active(&self, active: bool) -> Result<()> {
        self.connection.set_csi_active(active);
        if self.disco.has_feature("urn:xmpp:csi:0") {
            self.connection.send_csi_state()?;
        }
        Ok(())
    }

    /// Send a group chat message to a MUC room and record it locally.
    pub fn send_muc_message(&self, room: &Jid, nick: &str, body: &str) -> Result<MucMessage> {
        let id = Uuid::new_v4().to_string();
        let nicks: Vec<String> = self
            .mucs
            .get(room)
            .map(|room| {
                room.occupants()
                    .into_iter()
                    .map(|occupant| occupant.nick)
                    .collect()
            })
            .unwrap_or_default();
        let references = mention_references(body, room, &nicks);

        let mut stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "groupchat")
            .attr("to", room.to_string())
            .attr("id", id.clone())
            .append(Element::builder("body", ns::JABBER_CLIENT).append(body.to_string()))
            .append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id.clone()))
            .append(Element::builder("markable", "urn:xmpp:chat-markers:0"))
            .append(Element::builder("store", "urn:xmpp:hints"));
        for (uri, begin, end) in &references {
            stanza = stanza.append(
                Element::builder("reference", "urn:xmpp:reference:0")
                    .attr("type", "mention")
                    .attr("uri", uri.clone())
                    .attr("begin", begin.to_string())
                    .attr("end", end.to_string()),
            );
        }
        let stanza = stanza.build();

        self.connection.send_stanza_now(stanza)?;

        let mut msg = MucMessage::new(
            room.to_bare().to_string(),
            nick.to_string(),
            self.connection.jid(),
            true,
            Some(body.to_string()),
        );
        msg.id = id.clone();
        msg.origin_id = Some(id);
        msg.hints.store = true;
        msg.hints.markable = true;
        msg.mentions = references.into_iter().map(|(uri, _, _)| uri).collect();

        let mut room_ref = self.mucs.get_or_create(room);
        room_ref.add_message(msg.clone());

        Ok(msg)
    }

    /// Send a 1:1 message carrying an uploaded file (XEP-0363 + OOB + file metadata).
    pub fn send_file_message(
        &self,
        to: &Jid,
        url: &str,
        name: &str,
        mime: &str,
        size: u64,
    ) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();
        let stanza = file_stanza("chat", &to.to_string(), &id, url, name, mime, size);
        self.connection.send_stanza_now(stanza)?;

        let own_jid = self.connection.jid().unwrap_or_else(|| to.clone());
        let mut msg = ChatMessage::new(
            to.to_bare().to_string(),
            own_jid,
            to.clone(),
            true,
            Some(url.to_string()),
        );
        msg.id = id.clone();
        msg.origin_id = Some(id);
        msg.hints.store = true;
        msg.media.push(MediaAttachment {
            id: Uuid::new_v4().to_string(),
            url: url.to_string(),
            media_type: MediaType::from_mime(mime),
            name: Some(name.to_string()),
            size: Some(size),
            width: None,
            height: None,
            duration: None,
            thumbnail_url: None,
            blur_hash: None,
        });

        let mut chat = self.chats.get_or_create(to);
        chat.add_message(msg.clone());
        Ok(msg)
    }

    /// Send a groupchat message carrying an uploaded file.
    pub fn send_muc_file_message(
        &self,
        room: &Jid,
        nick: &str,
        url: &str,
        name: &str,
        mime: &str,
        size: u64,
    ) -> Result<MucMessage> {
        let id = Uuid::new_v4().to_string();
        let stanza = file_stanza("groupchat", &room.to_string(), &id, url, name, mime, size);
        self.connection.send_stanza_now(stanza)?;

        let mut msg = MucMessage::new(
            room.to_bare().to_string(),
            nick.to_string(),
            self.connection.jid(),
            true,
            Some(url.to_string()),
        );
        msg.id = id.clone();
        msg.origin_id = Some(id);
        msg.hints.store = true;
        msg.media.push(MediaAttachment {
            id: Uuid::new_v4().to_string(),
            url: url.to_string(),
            media_type: MediaType::from_mime(mime),
            name: Some(name.to_string()),
            size: Some(size),
            width: None,
            height: None,
            duration: None,
            thumbnail_url: None,
            blur_hash: None,
        });

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
        let mut bundles = self.omemo_bundles(to);
        if let Some(local) = self.omemo.local() {
            if !bundles.iter().any(|b| b.device_id == local.device_id) {
                bundles.push(local.bundle());
            }
        }
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
            .append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id.clone()))
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
        msg.receipt = crate::chat::MessageReceipt::Sent;
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
            .append(Element::builder("no-store", "urn:xmpp:hints"))
            .append(Element::builder("no-copy", "urn:xmpp:hints"))
            .build();
        self.connection.send_stanza_now(stanza)
    }

    /// Mark a received message as displayed (XEP-0333).
    pub fn send_displayed(&self, to: &Jid, message_id: &str) -> Result<()> {
        self.send_marker(to, "displayed", message_id)
    }

    /// Send a reply (XEP-0461) to a specific message and record it locally.
    pub fn send_reply(&self, to: &Jid, body: &str, reply_to_id: &str) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", id.clone())
            .append(Element::builder("body", ns::JABBER_CLIENT).append(body.to_string()))
            .append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id.clone()))
            .append(
                Element::builder("reply", "urn:xmpp:reply:0")
                    .attr("id", reply_to_id.to_string())
                    .attr("to", to.to_bare().to_string()),
            )
            .append(Element::builder("request", "urn:xmpp:receipts"))
            .append(Element::builder("markable", "urn:xmpp:chat-markers:0"))
            .append(Element::builder("store", "urn:xmpp:hints"))
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
        msg.id = id.clone();
        msg.origin_id = Some(id);
        msg.hints.store = true;
        msg.hints.markable = true;
        msg.receipt = crate::chat::MessageReceipt::Sent;
        msg.reply_to = Some(reply_to_id.to_string());

        let mut chat = self.chats.get_or_create(to);
        chat.add_message(msg.clone());
        Ok(msg)
    }

    /// Request Bits of Binary content from the entity that sent a sticker.
    pub fn request_bob(&self, from: &Jid, cid: &str) -> Result<()> {
        let iq = crate::bob::request(&format!("bob-{}", Uuid::new_v4()), &from.to_string(), cid);
        self.connection.send_stanza_now(iq)
    }

    /// Send a sticker as a XEP-0231 BOB image in an XHTML-IM message.
    pub fn send_sticker(
        &self,
        to: &Jid,
        bytes: &[u8],
        mime: &str,
        name: &str,
    ) -> Result<ChatMessage> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let cid = crate::bob::cid_for_bytes(bytes);
        crate::bob::register(crate::bob::BobContent {
            cid: cid.clone(),
            mime: mime.to_string(),
            data: STANDARD.encode(bytes),
            max_age: 3600,
        });
        let id = Uuid::new_v4().to_string();
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", id.clone())
            .append(Element::builder("body", ns::JABBER_CLIENT).append("[Sticker]".to_string()))
            .append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id.clone()))
            .append(Element::builder("store", "urn:xmpp:hints"))
            .append(crate::bob::sticker_html(&cid, name))
            .build();
        self.connection.send_stanza_now(stanza)?;

        let own = self.connection.jid().unwrap_or_else(|| to.clone());
        let mut msg = ChatMessage::new(
            to.to_bare().to_string(),
            own,
            to.clone(),
            true,
            Some("[Sticker]".to_string()),
        );
        msg.id = id.clone();
        msg.origin_id = Some(id);
        msg.hints.store = true;
        msg.sticker_cid = Some(cid);
        msg.sticker_alt = Some(name.to_string());
        let mut chat = self.chats.get_or_create(to);
        chat.add_message(msg.clone());
        Ok(msg)
    }

    /// Send a sticker into a room as a XEP-0231 BOB image.
    pub fn send_muc_sticker(
        &self,
        room: &Jid,
        nick: &str,
        bytes: &[u8],
        mime: &str,
        name: &str,
    ) -> Result<MucMessage> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let cid = crate::bob::cid_for_bytes(bytes);
        crate::bob::register(crate::bob::BobContent {
            cid: cid.clone(),
            mime: mime.to_string(),
            data: STANDARD.encode(bytes),
            max_age: 3600,
        });
        let id = Uuid::new_v4().to_string();
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "groupchat")
            .attr("to", room.to_string())
            .attr("id", id.clone())
            .append(Element::builder("body", ns::JABBER_CLIENT).append("[Sticker]".to_string()))
            .append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id.clone()))
            .append(Element::builder("store", "urn:xmpp:hints"))
            .append(crate::bob::sticker_html(&cid, name))
            .build();
        self.connection.send_stanza_now(stanza)?;

        let mut msg = MucMessage::new(
            room.to_bare().to_string(),
            nick.to_string(),
            self.connection.jid(),
            true,
            Some("[Sticker]".to_string()),
        );
        msg.id = id.clone();
        msg.origin_id = Some(id);
        msg.hints.store = true;
        msg.sticker_cid = Some(cid);
        msg.sticker_alt = Some(name.to_string());
        let mut room_ref = self.mucs.get_or_create(room);
        room_ref.add_message(msg.clone());
        Ok(msg)
    }

    /// Forward an existing 1:1 message (XEP-0297) and record it locally.
    pub fn send_forward(&self, to: &Jid, original: &ChatMessage) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();
        let stanza = forward_stanza("chat", &to.to_string(), &id, original);
        self.connection.send_stanza_now(stanza)?;

        let own = self.connection.jid().unwrap_or_else(|| to.clone());
        let mut msg = ChatMessage::new(
            to.to_bare().to_string(),
            own,
            to.clone(),
            true,
            Some(original.display_body()),
        );
        msg.id = id.clone();
        msg.origin_id = Some(id);
        msg.hints.store = true;
        msg.forward_info = Some(crate::chat::ForwardInfo {
            original_jid: original.from_jid.clone(),
            original_timestamp: original.timestamp,
            original_id: original.id.clone(),
        });
        msg.media = original.media.clone();
        let mut chat = self.chats.get_or_create(to);
        chat.add_message(msg.clone());
        Ok(msg)
    }

    /// Forward an existing message into a room (XEP-0297).
    pub fn send_muc_forward(
        &self,
        room: &Jid,
        nick: &str,
        original: &ChatMessage,
    ) -> Result<MucMessage> {
        let id = Uuid::new_v4().to_string();
        let stanza = forward_stanza("groupchat", &room.to_string(), &id, original);
        self.connection.send_stanza_now(stanza)?;

        let mut msg = MucMessage::new(
            room.to_bare().to_string(),
            nick.to_string(),
            self.connection.jid(),
            true,
            Some(original.display_body()),
        );
        msg.id = id.clone();
        msg.origin_id = Some(id);
        msg.hints.store = true;
        msg.forward_info = Some(crate::chat::ForwardInfo {
            original_jid: original.from_jid.clone(),
            original_timestamp: original.timestamp,
            original_id: original.id.clone(),
        });
        msg.media = original.media.clone();
        let mut room_ref = self.mucs.get_or_create(room);
        room_ref.add_message(msg.clone());
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

    /// Add or replace the local user's reactions in a room (XEP-0444).
    pub fn send_muc_reaction(&self, room: &Jid, message_id: &str, emojis: &[&str]) -> Result<()> {
        let mut reactions = Element::builder("reactions", "urn:xmpp:reactions:0")
            .attr("id", message_id.to_string());
        for emoji in emojis {
            reactions = reactions.append(
                Element::builder("reaction", "urn:xmpp:reactions:0").append(emoji.to_string()),
            );
        }
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "groupchat")
            .attr("to", room.to_bare().to_string())
            .attr("id", format!("react-{}", Uuid::new_v4()))
            .append(reactions)
            .build();
        self.connection.send_stanza_now(stanza)?;

        if let Some(mut room_ref) = self.mucs.get(room) {
            let nick = room_ref.self_nick().unwrap_or_else(|| "me".to_string());
            let owned: Vec<String> = emojis.iter().map(|emoji| (*emoji).to_string()).collect();
            room_ref.set_reactions_from(message_id, &nick, &owned);
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

    /// Query disco#info for an entity, including XEP-0444 reaction restrictions.
    pub fn request_disco_info(&self, jid: &Jid) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", jid.to_string())
            .attr("id", format!("disco-info-{}", Uuid::new_v4()))
            .append(Element::builder(
                "query",
                "http://jabber.org/protocol/disco#info",
            ))
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
                        .append(
                            Element::builder("set", "http://jabber.org/protocol/rsm").append(
                                Element::builder("max", "http://jabber.org/protocol/rsm")
                                    .append(max_items.to_string()),
                            ),
                        ),
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
                        .append(
                            Element::builder("set", "http://jabber.org/protocol/rsm").append(
                                Element::builder("max", "http://jabber.org/protocol/rsm")
                                    .append(max_items.to_string()),
                            ),
                        ),
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

        self.mucs.set_subject(
            &room.to_bare().to_string(),
            subject.to_string(),
            self.connection.jid(),
        );
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

    /// Publish a PEP user avatar (XEP-0084): metadata and data nodes.
    pub fn publish_avatar(&self, data: &[u8], mime: &str) -> Result<()> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        use sha1::{Digest, Sha1};

        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        let digest = Sha1::digest(data);
        let id: String = digest.iter().map(|b| format!("{:02x}", b)).collect();

        let metadata = Element::builder("metadata", avatar::NS_AVATAR_METADATA)
            .append(
                Element::builder("info", avatar::NS_AVATAR_METADATA)
                    .attr("id", id.clone())
                    .attr("type", mime.to_string())
                    .attr("bytes", data.len().to_string()),
            )
            .build();
        self.publish_pep(&own, avatar::NS_AVATAR_METADATA, Some(&id), metadata)?;

        let payload = Element::builder("data", avatar::NS_AVATAR_DATA)
            .append(STANDARD.encode(data))
            .build();
        self.publish_pep(&own, avatar::NS_AVATAR_DATA, Some(&id), payload)?;

        let mut item = self
            .roster
            .get(&own.clone().into())
            .unwrap_or_else(|| crate::roster::RosterItem::new(own.clone().into()));
        item.avatar_hash = Some(id);
        item.avatar_mime = Some(mime.to_string());
        item.avatar_data = Some(STANDARD.encode(data));
        self.roster.upsert(item);
        Ok(())
    }

    /// Publish our current tune (XEP-0107).
    pub fn publish_tune(
        &self,
        artist: Option<&str>,
        title: Option<&str>,
        source: Option<&str>,
        length: Option<u32>,
    ) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        self.publish_pep(
            &own,
            crate::pep::NS_TUNE,
            Some(&Uuid::new_v4().to_string()),
            crate::pep::tune(artist, title, source, length),
        )
    }

    /// Publish our current mood (XEP-0118).
    pub fn publish_mood(&self, value: &str, text: Option<&str>) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        self.publish_pep(
            &own,
            crate::pep::NS_MOOD,
            Some(&Uuid::new_v4().to_string()),
            crate::pep::mood(value, text),
        )
    }

    /// Publish our current activity (XEP-0108).
    pub fn publish_activity(
        &self,
        general: &str,
        specific: Option<&str>,
        text: Option<&str>,
    ) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        self.publish_pep(
            &own,
            crate::pep::NS_ACTIVITY,
            Some(&Uuid::new_v4().to_string()),
            crate::pep::activity(general, specific, text),
        )
    }

    /// Publish our current location (XEP-0080).
    pub fn publish_location(
        &self,
        latitude: f64,
        longitude: f64,
        text: Option<&str>,
    ) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        self.publish_pep(
            &own,
            crate::pep::NS_LOCATION,
            Some(&Uuid::new_v4().to_string()),
            crate::pep::location(latitude, longitude, text),
        )
    }

    /// Publish a vCard4 payload (XEP-0292) on the PEP node.
    pub fn publish_vcard4(&self, vcard: Element) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        self.publish_pep(
            &own,
            crate::pep::NS_VCARD4_NODE,
            Some(&Uuid::new_v4().to_string()),
            vcard,
        )
    }

    /// Fetch all user-activity nodes for a contact.
    pub fn fetch_user_activity(&self, jid: &Jid) -> Result<()> {
        for node in [
            crate::pep::NS_TUNE,
            crate::pep::NS_MOOD,
            crate::pep::NS_ACTIVITY,
            crate::pep::NS_LOCATION,
            crate::pep::NS_VCARD4_NODE,
        ] {
            self.fetch_pep_items(jid, node, 1)?;
        }
        Ok(())
    }

    /// Request a contact's PEP avatar metadata (XEP-0084).
    pub fn fetch_avatar(&self, jid: &Jid) -> Result<()> {
        self.fetch_pep_items(jid, avatar::NS_AVATAR_METADATA, 1)
    }

    pub fn fetch_avatar_str(&self, jid: &str) -> Result<()> {
        let jid: Jid = jid
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.fetch_avatar(&jid)
    }

    /// Request a contact's PEP avatar data node (XEP-0084).
    pub fn fetch_avatar_data(&self, jid: &Jid) -> Result<()> {
        self.fetch_pep_items(jid, avatar::NS_AVATAR_DATA, 1)
    }

    /// Update our own vCard (XEP-0054). Unset fields are left out.
    pub fn set_vcard(
        &self,
        nickname: Option<&str>,
        full_name: Option<&str>,
        avatar: Option<(&[u8], &str)>,
    ) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        let existing = self.roster.get(&own);
        let existing_avatar = existing
            .as_ref()
            .and_then(|i| i.avatar_data.clone().zip(i.avatar_mime.clone()));
        let mut vcard = Element::builder("vCard", "vcard-temp");
        let name = full_name
            .filter(|n| !n.trim().is_empty())
            .or_else(|| existing.as_ref().and_then(|i| i.name.as_deref()));
        if let Some(name) = name {
            vcard = vcard.append(Element::builder("FN", "vcard-temp").append(name.trim()));
        }
        if let Some(nick) = nickname.filter(|n| !n.trim().is_empty()) {
            vcard = vcard.append(Element::builder("NICKNAME", "vcard-temp").append(nick.trim()));
        }
        if let Some((data, mime)) = avatar {
            use base64::{engine::general_purpose::STANDARD, Engine as _};
            vcard = vcard.append(
                Element::builder("PHOTO", "vcard-temp")
                    .append(Element::builder("TYPE", "vcard-temp").append(mime))
                    .append(Element::builder("BINVAL", "vcard-temp").append(STANDARD.encode(data))),
            );
        } else if let Some((encoded, mime)) = existing_avatar {
            vcard = vcard.append(
                Element::builder("PHOTO", "vcard-temp")
                    .append(Element::builder("TYPE", "vcard-temp").append(mime))
                    .append(Element::builder("BINVAL", "vcard-temp").append(encoded)),
            );
        }
        self.update_vcard(vcard.build())
    }

    /// Merge fields into our own vCard (XEP-0054) without dropping existing ones.
    ///
    /// # Errors
    ///
    /// Returns an error when not connected or the stanza cannot be sent.
    pub fn update_vcard(&self, patch: Element) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        let existing = self
            .roster
            .get(&own)
            .and_then(|item| item.vcard)
            .and_then(|xml| xml.parse::<Element>().ok());
        let merged = crate::vcard::merge_vcard(existing.as_ref(), &patch);
        let xml = crate::vcard::to_xml(&merged);

        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", own.to_string())
            .attr("id", format!("vcard-set-{}", Uuid::new_v4()))
            .append(merged.clone())
            .build();
        self.connection.send_stanza_now(iq)?;

        let mut item = self
            .roster
            .get(&own)
            .unwrap_or_else(|| crate::roster::RosterItem::new(own.clone().into()));
        if let Some(name) = crate::vcard::field_text(&merged, "FN") {
            item.name = Some(name);
        }
        if let Some(photo) = crate::vcard::field(&merged, "PHOTO") {
            if let Some(data) = crate::vcard::field_text(photo, "BINVAL") {
                item.avatar_data = Some(data);
            }
            if let Some(mime) = crate::vcard::field_text(photo, "TYPE") {
                item.avatar_mime = Some(mime);
            }
        }
        item.vcard = Some(xml);
        self.roster.upsert(item);
        Ok(())
    }

    /// Merge fields into our own vCard4 (XEP-0292) and publish it.
    ///
    /// # Errors
    ///
    /// Returns an error when not connected or the PEP publish fails.
    pub fn update_vcard4(&self, patch: Element) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        let existing = self
            .roster
            .get(&own)
            .and_then(|item| item.vcard4)
            .and_then(|xml| xml.parse::<Element>().ok());
        let merged = crate::vcard4::merge_vcard4(existing.as_ref(), &patch);
        self.publish_vcard4(merged.clone())?;

        let xml = crate::vcard::to_xml(&merged);
        let mut item = self
            .roster
            .get(&own)
            .unwrap_or_else(|| crate::roster::RosterItem::new(own.clone().into()));
        if let Some(name) = crate::vcard4::VCard4::from_element(&merged).full_name {
            item.name = Some(name);
        }
        item.vcard4 = Some(xml);
        self.roster.upsert(item);
        Ok(())
    }

    /// The last vCard we stored for a contact.
    #[must_use]
    pub fn stored_vcard(&self, jid: &Jid) -> Option<Element> {
        self.roster
            .get(jid)
            .and_then(|item| item.vcard)
            .and_then(|xml| xml.parse::<Element>().ok())
    }

    /// Ask a contact (or the server) for their last activity (XEP-0012).
    pub fn request_last_activity(&self, jid: &Jid) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", jid.to_bare().to_string())
            .attr("id", format!("last-{}", Uuid::new_v4()))
            .append(Element::builder("query", "jabber:iq:last"))
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Broadcast a presence update (XEP-0045 style menu: show/status/priority).
    ///
    /// # Errors
    ///
    /// Returns an error when the stanza cannot be queued.
    pub fn set_presence(
        &self,
        show: Option<&str>,
        status: Option<&str>,
        priority: Option<i32>,
    ) -> Result<()> {
        let mut config = self.connection.config();
        config.presence_show = show.map(str::to_string);
        config.presence_status = status.map(str::to_string);
        if let Some(priority) = priority {
            config.priority = priority;
        }
        self.connection.set_config(config.clone());

        let nick = self
            .connection
            .jid()
            .and_then(|jid| self.roster.get(&jid.to_bare()))
            .and_then(|item| item.vcard_field("NICKNAME"));
        self.connection
            .send_stanza_now(crate::presence::presence_element(
                show,
                status,
                config.priority,
                nick.as_deref(),
            ))
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

    /// Fetch the owner configuration form for a room (XEP-0045).
    pub async fn room_config(&self, room: &Jid) -> Result<crate::data_form::DataForm> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", room.to_bare().to_string())
            .attr("id", format!("room-config-{}", Uuid::new_v4()))
            .append(Element::builder(
                "query",
                "http://jabber.org/protocol/muc#owner",
            ))
            .build();
        let reply = self.iq_request(iq).await?;
        let query = reply
            .get_child("query", "http://jabber.org/protocol/muc#owner")
            .ok_or_else(|| XmppError::ProtocolError("no room configuration returned".into()))?;
        let form = query
            .get_child("x", crate::data_form::NS_DATA_FORMS)
            .ok_or_else(|| XmppError::ProtocolError("room configuration has no form".into()))?;
        Ok(crate::data_form::parse(form))
    }

    /// Submit a room configuration form (XEP-0045).
    pub async fn submit_room_config(
        &self,
        room: &Jid,
        form: Option<&Element>,
        values: &[(String, String)],
    ) -> Result<()> {
        let payload = crate::data_form::submit(form, values);
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", room.to_bare().to_string())
            .attr("id", format!("room-config-set-{}", Uuid::new_v4()))
            .append(
                Element::builder("query", "http://jabber.org/protocol/muc#owner").append(payload),
            )
            .build();
        self.iq_request(iq).await?;
        Ok(())
    }

    /// Destroy a room, optionally with a reason (XEP-0045).
    pub async fn destroy_room(&self, room: &Jid, reason: Option<&str>) -> Result<()> {
        let mut destroy = Element::builder("destroy", "http://jabber.org/protocol/muc#owner");
        if let Some(reason) = reason.filter(|reason| !reason.trim().is_empty()) {
            destroy = destroy.append(
                Element::builder("reason", "http://jabber.org/protocol/muc#owner")
                    .append(reason.to_string()),
            );
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", room.to_bare().to_string())
            .attr("id", format!("room-destroy-{}", Uuid::new_v4()))
            .append(
                Element::builder("query", "http://jabber.org/protocol/muc#owner").append(destroy),
            )
            .build();
        self.iq_request(iq).await?;
        self.mucs.remove(room);
        Ok(())
    }

    /// Leave a MUC room.
    pub fn leave_room(&self, room: &Jid) -> Result<()> {
        let bare = room.to_bare().to_string();
        let nick = self
            .mucs
            .get_by_bare(&bare)
            .and_then(|r| r.self_nick())
            .unwrap_or_else(|| "me".to_string());
        let presence = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("to", format!("{}/{}", room.to_bare(), nick))
            .attr("type", "unavailable")
            .build();
        self.connection.send_stanza_now(presence)?;
        if let Some(mut r) = self.mucs.get_by_bare(&bare) {
            r.leave(None)?;
        }
        self.mucs.remove(room);
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

    /// Replace a contact's roster groups (XEP-0237-style roster edit).
    ///
    /// # Errors
    ///
    /// Returns an error when the roster stanza cannot be queued.
    pub fn set_contact_groups(&self, jid: &Jid, groups: &[String]) -> Result<()> {
        let mut item = self
            .roster
            .get(jid)
            .unwrap_or_else(|| crate::roster::RosterItem::new(jid.clone()));
        item.groups = groups.to_vec();
        self.roster.upsert(item);

        let mut entry =
            Element::builder("item", "jabber:iq:roster").attr("jid", jid.to_bare().to_string());
        for group in groups {
            entry = entry
                .append(Element::builder("group", "jabber:iq:roster").append(group.to_string()));
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("id", format!("roster-groups-{}", Uuid::new_v4()))
            .append(Element::builder("query", "jabber:iq:roster").append(entry))
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Set a contact's display name.
    ///
    /// # Errors
    ///
    /// Returns an error when the roster stanza cannot be queued.
    pub fn set_contact_name(&self, jid: &Jid, name: Option<&str>) -> Result<()> {
        let mut item = self
            .roster
            .get(jid)
            .unwrap_or_else(|| crate::roster::RosterItem::new(jid.clone()));
        item.name = name.map(str::to_string);
        self.roster.upsert(item);

        let mut entry =
            Element::builder("item", "jabber:iq:roster").attr("jid", jid.to_bare().to_string());
        if let Some(name) = name {
            entry = entry.attr("name", name);
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("id", format!("roster-name-{}", Uuid::new_v4()))
            .append(Element::builder("query", "jabber:iq:roster").append(entry))
            .build();
        self.connection.send_stanza_now(iq)
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
        self.query_mam_page(with, max, None)
    }

    /// Query the message archive ending before an archive id (older page).
    pub fn query_mam_before(&self, with: &Jid, max: u32, before: &str) -> Result<()> {
        self.query_mam_page(with, max, Some(before))
    }

    pub fn query_mam_before_str(&self, with: &str, max: u32, before: &str) -> Result<()> {
        let jid: Jid = with
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.query_mam_before(&jid, max, before)
    }

    fn query_mam_page(&self, with: &Jid, max: u32, before: Option<&str>) -> Result<()> {
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

        let mut rsm = Element::builder("set", "http://jabber.org/protocol/rsm").append(
            Element::builder("max", "http://jabber.org/protocol/rsm").append(max.to_string()),
        );
        if let Some(before) = before {
            rsm = rsm.append(
                Element::builder("before", "http://jabber.org/protocol/rsm").append(before),
            );
        }
        let query = Element::builder("query", "urn:xmpp:mam:2")
            .attr("queryid", query_id.clone())
            .append(fields)
            .append(rsm);

        let iq_id = format!("mam-{}", Uuid::new_v4());
        self.mam
            .register(&query_id, with.to_bare().to_string(), false);
        self.mam.register(&iq_id, with.to_bare().to_string(), false);
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("id", iq_id)
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
        self.query_muc_mam_page(room, max, None)
    }

    /// Query a room's archive ending before an archive id (older page).
    pub fn query_muc_mam_before(&self, room: &Jid, max: u32, before: &str) -> Result<()> {
        self.query_muc_mam_page(room, max, Some(before))
    }

    pub fn query_muc_mam_before_str(&self, room: &str, max: u32, before: &str) -> Result<()> {
        let jid: Jid = room
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.query_muc_mam_before(&jid, max, before)
    }

    fn query_muc_mam_page(&self, room: &Jid, max: u32, before: Option<&str>) -> Result<()> {
        let query_id = format!("muc-q-{}", Uuid::new_v4());
        let fields = Element::builder("x", "jabber:x:data")
            .attr("type", "submit")
            .append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", "FORM_TYPE")
                    .attr("type", "hidden")
                    .append(Element::builder("value", "jabber:x:data").append("urn:xmpp:mam:2")),
            );
        let mut rsm = Element::builder("set", "http://jabber.org/protocol/rsm").append(
            Element::builder("max", "http://jabber.org/protocol/rsm").append(max.to_string()),
        );
        if let Some(before) = before {
            rsm = rsm.append(
                Element::builder("before", "http://jabber.org/protocol/rsm").append(before),
            );
        }
        let query = Element::builder("query", "urn:xmpp:mam:2")
            .attr("queryid", query_id.clone())
            .append(fields)
            .append(rsm);
        let iq_id = format!("mam-muc-{}", Uuid::new_v4());
        self.mam
            .register(&query_id, room.to_bare().to_string(), true);
        self.mam.register(&iq_id, room.to_bare().to_string(), true);
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", room.to_bare().to_string())
            .attr("id", iq_id)
            .append(query)
            .build();
        self.connection.send_stanza_now(iq)
    }
}

/// Configuration for the XMPP client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    pub direct_tls: bool,
    pub presence_show: Option<String>,
    pub presence_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
            direct_tls: false,
            presence_show: None,
            presence_status: None,
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
            direct_tls: config.direct_tls,
            presence_show: config.presence_show,
            presence_status: config.presence_status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minidom::NSChoice;

    #[test]
    fn file_stanza_carries_oob_and_metadata() {
        let stanza = file_stanza(
            "chat",
            "bob@example.org",
            "m1",
            "https://files.example/a.png",
            "a.png",
            "image/png",
            42,
        );
        assert_eq!(stanza.name(), "message");
        assert_eq!(stanza.attr("type"), Some("chat"));
        assert_eq!(
            stanza
                .get_child("body", NSChoice::Any)
                .map(minidom::Element::text)
                .as_deref(),
            Some("https://files.example/a.png")
        );
        let oob = stanza.get_child("x", "jabber:x:oob").unwrap();
        assert_eq!(
            oob.get_child("url", "jabber:x:oob")
                .map(minidom::Element::text)
                .as_deref(),
            Some("https://files.example/a.png")
        );
        let file = stanza
            .get_child("file", "urn:xmpp:file:metadata:0")
            .unwrap();
        assert_eq!(
            file.get_child("name", "urn:xmpp:file:metadata:0")
                .map(minidom::Element::text)
                .as_deref(),
            Some("a.png")
        );
        assert_eq!(
            file.get_child("size", "urn:xmpp:file:metadata:0")
                .map(minidom::Element::text)
                .as_deref(),
            Some("42")
        );
    }

    #[test]
    fn mention_references_use_character_offsets() {
        let room: Jid = "room@conference.example.org".parse().unwrap();
        let references = mention_references(
            "hi @bob and @alice!",
            &room,
            &["bob".to_string(), "alice".to_string()],
        );
        assert_eq!(
            references,
            vec![
                ("xmpp:room@conference.example.org/bob".into(), 3, 7),
                ("xmpp:room@conference.example.org/alice".into(), 12, 18),
            ]
        );
    }

    #[test]
    fn forward_stanza_wraps_the_original() {
        let original = ChatMessage::new(
            "alice@example.org".into(),
            "alice@example.org".parse().unwrap(),
            "bob@example.org".parse().unwrap(),
            false,
            Some("original".into()),
        );
        let stanza = forward_stanza("chat", "carol@example.org", "f1", &original);
        assert_eq!(stanza.attr("type"), Some("chat"));
        let forwarded = stanza.get_child("forwarded", "urn:xmpp:forward:0").unwrap();
        assert!(forwarded.get_child("delay", "urn:xmpp:delay").is_some());
        let inner = forwarded
            .get_child("message", NSChoice::Any)
            .expect("inner message");
        assert_eq!(inner.attr("from"), Some("alice@example.org"));
        assert_eq!(
            inner
                .get_child("body", NSChoice::Any)
                .map(minidom::Element::text)
                .as_deref(),
            Some("original")
        );
    }
}
