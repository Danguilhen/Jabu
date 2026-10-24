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
pub mod spaces;
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
pub use spaces::{
    Space, SpaceAffiliation, SpaceDirectory, SpaceId, SpaceManager, SpaceNotify, SpaceRoom,
    SpaceSubscription,
};
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
    pub spaces: Arc<SpaceManager>,
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

fn data_form_field(var: &str, value: &str, hidden: bool) -> Element {
    let mut field = Element::builder("field", "jabber:x:data").attr("var", var);
    if hidden {
        field = field.attr("type", "hidden");
    }
    field
        .append(Element::builder("value", "jabber:x:data").append(value))
        .build()
}

pub(crate) fn form_value(form: &Element, var: &str) -> Option<String> {
    form.children()
        .filter(|child| child.name() == "field")
        .find(|field| field.attr("var") == Some(var))
        .and_then(|field| field.children().find(|child| child.name() == "value"))
        .map(|value| value.text())
}

pub(crate) fn parse_space_directories(element: &Element) -> Vec<crate::spaces::SpaceDirectory> {
    element
        .children()
        .filter(|child| child.name() == "directory")
        .enumerate()
        .filter_map(|(index, directory)| {
            let id = directory.attr("id")?;
            let name = directory.attr("title").unwrap_or(id).to_string();
            Some(crate::spaces::SpaceDirectory {
                id: id.to_string(),
                name,
                order: index,
            })
        })
        .collect()
}

pub(crate) fn parse_space_room(
    item_id: &str,
    conference: &Element,
) -> Option<crate::spaces::SpaceRoom> {
    if item_id.is_empty() {
        return None;
    }
    let name = conference
        .attr("name")
        .filter(|name| !name.is_empty())
        .unwrap_or(item_id)
        .to_string();
    let autojoin = conference.attr("autojoin") == Some("true");
    let nick = conference
        .get_child("nick", minidom::NSChoice::Any)
        .map(|nick| nick.text())
        .filter(|nick| !nick.is_empty());
    let mut notify = crate::spaces::SpaceNotify::Always;
    let mut pinned = false;
    let mut directory_id = None;
    let mut weight = 0;
    if let Some(extensions) = conference.get_child("extensions", minidom::NSChoice::Any) {
        if let Some(notify_node) = extensions.get_child("notify", minidom::NSChoice::Any) {
            if notify_node
                .get_child("never", minidom::NSChoice::Any)
                .is_some()
            {
                notify = crate::spaces::SpaceNotify::Never;
            } else if notify_node
                .get_child("on-mention", minidom::NSChoice::Any)
                .is_some()
            {
                notify = crate::spaces::SpaceNotify::Mentions;
            }
        }
        pinned = extensions
            .get_child("pinned", minidom::NSChoice::Any)
            .is_some();
        if let Some(hierarchy) = extensions.get_child("hierarchy", minidom::NSChoice::Any) {
            directory_id = hierarchy.attr("directory-id").map(str::to_string);
            weight = hierarchy
                .attr("weight")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
        }
    }
    Some(crate::spaces::SpaceRoom {
        conference: item_id.to_string(),
        name,
        nick,
        autojoin,
        directory_id,
        notify,
        pinned,
        weight,
    })
}

fn space_room_item(room: &crate::spaces::SpaceRoom) -> Element {
    let mut conference =
        Element::builder("conference", crate::spaces::NS_BOOKMARKS).attr("name", room.name.clone());
    if room.autojoin {
        conference = conference.attr("autojoin", "true");
    }
    if let Some(nick) = room.nick.as_ref().filter(|nick| !nick.is_empty()) {
        conference = conference
            .append(Element::builder("nick", crate::spaces::NS_BOOKMARKS).append(nick.clone()));
    }

    let notify_name = match room.notify {
        crate::spaces::SpaceNotify::Always => "always",
        crate::spaces::SpaceNotify::Mentions => "on-mention",
        crate::spaces::SpaceNotify::Never => "never",
    };
    let mut extensions = Element::builder("extensions", crate::spaces::NS_BOOKMARKS).append(
        Element::builder("notify", crate::spaces::NS_NOTIFICATIONS).append(Element::builder(
            notify_name,
            crate::spaces::NS_NOTIFICATIONS,
        )),
    );
    if room.pinned {
        extensions = extensions.append(Element::builder("pinned", crate::spaces::NS_PINNED));
    }
    if room.directory_id.is_some() || room.weight > 0 {
        let mut hierarchy = Element::builder("hierarchy", crate::spaces::NS_SPACE_DIRECTORIES)
            .attr("weight", room.weight.to_string());
        if let Some(directory) = room.directory_id.as_ref() {
            hierarchy = hierarchy.attr("directory-id", directory.clone());
        }
        extensions = extensions.append(hierarchy);
    }
    conference = conference.append(extensions);

    Element::builder("item", "http://jabber.org/protocol/pubsub")
        .attr("id", room.conference.clone())
        .append(conference)
        .build()
}

fn affiliation_slug(affiliation: crate::spaces::SpaceAffiliation) -> &'static str {
    match affiliation {
        crate::spaces::SpaceAffiliation::Owner => "owner",
        crate::spaces::SpaceAffiliation::Admin => "admin",
        crate::spaces::SpaceAffiliation::Member => "member",
        crate::spaces::SpaceAffiliation::Outcast => "outcast",
        crate::spaces::SpaceAffiliation::None => "none",
    }
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

    /// Mute or unmute the remote audio (speaker output).
    pub async fn set_call_speaker_muted(&self, sid: &str, muted: bool) -> Result<()> {
        self.jingle.set_speaker_muted(sid, muted).await
    }

    /// Enable or disable the local camera. Enabling it on an audio call adds
    /// a video content to the running Jingle session.
    pub async fn set_call_camera(&self, sid: &str, enabled: bool) -> Result<()> {
        self.jingle.set_camera(sid, enabled).await
    }

    /// Local cameras that can be selected for outgoing video.
    #[must_use]
    pub fn call_camera_devices(&self) -> Vec<crate::jingle::CameraDevice> {
        self.jingle.camera_devices()
    }

    /// Enumerate local audio input devices.
    #[must_use]
    pub fn call_audio_devices(&self) -> Vec<crate::jingle::AudioDevice> {
        self.jingle.audio_devices()
    }

    /// Switch the running call to the next local camera.
    pub async fn switch_call_camera(&self, sid: &str) -> Result<()> {
        self.jingle.switch_camera(sid).await
    }

    /// Select a specific local camera for the running call.
    pub async fn set_call_camera_device(&self, sid: &str, device_id: Option<&str>) -> Result<()> {
        self.jingle.set_camera_device(sid, device_id).await
    }

    /// Remember the camera to use for future calls and switch active video calls.
    pub async fn set_default_call_camera(&self, device_id: Option<&str>) -> Result<()> {
        self.jingle.set_preferred_camera(device_id).await
    }

    /// Select the microphone for future calls.
    pub async fn set_default_call_audio(&self, device_id: Option<&str>) -> Result<()> {
        self.jingle.set_audio_preference(device_id);
        Ok(())
    }

    /// Remember the microphone to use for future calls without switching a call.
    pub fn set_call_audio_preference(&self, device_id: Option<&str>) {
        self.jingle.set_audio_preference(device_id);
    }

    /// Remember the camera to use for future calls without switching a call.
    pub fn set_call_camera_preference(&self, device_id: Option<&str>) {
        self.jingle.set_camera_preference(device_id);
    }

    /// Latest locally captured camera frame, if available.
    #[must_use]
    pub fn call_preview(&self, sid: &str) -> Option<crate::jingle::CallPreview> {
        self.jingle.preview_frame(sid)
    }

    /// Latest received remote video frame, if available.
    #[must_use]
    pub fn call_remote_preview(&self, sid: &str) -> Option<crate::jingle::CallPreview> {
        self.jingle.remote_preview_frame(sid)
    }

    /// Start sharing the local screen on an active call.
    pub async fn start_call_screen_share(&self, sid: &str) -> Result<()> {
        self.jingle.start_screen_share(sid).await
    }

    /// Stop sharing the local screen.
    pub async fn stop_call_screen_share(&self, sid: &str) -> Result<()> {
        self.jingle.stop_screen_share(sid).await
    }

    /// Latest locally captured screen frame, if available.
    #[must_use]
    pub fn call_screen_preview(&self, sid: &str) -> Option<crate::jingle::CallPreview> {
        self.jingle.screen_preview_frame(sid)
    }

    /// Latest received remote screen frame, if available.
    #[must_use]
    pub fn call_remote_screen(&self, sid: &str) -> Option<crate::jingle::CallPreview> {
        self.jingle.remote_screen_frame(sid)
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

    /// Resolve the pubsub service used to host spaces (XEP-0503).
    ///
    /// The service discovered through `disco#info` wins; otherwise the first
    /// pubsub component is probed, then the conventional `pubsub.<domain>`.
    ///
    /// # Errors
    ///
    /// Returns an error when no pubsub service can be found.
    pub async fn resolve_spaces_service(&self) -> Result<Jid> {
        if let Some(service) = self.disco.spaces_service() {
            if let Ok(jid) = service.parse::<Jid>() {
                return Ok(Jid::from(jid.to_bare()));
            }
        }
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let mut candidates = self.disco.pubsub_services();
        candidates.push(format!("pubsub.{}", own.domain()));
        for candidate in candidates {
            let Ok(jid) = candidate.parse::<Jid>() else {
                continue;
            };
            let iq = Element::builder("iq", ns::JABBER_CLIENT)
                .attr("type", "get")
                .attr("to", jid.to_bare().to_string())
                .attr("id", format!("spaces-probe-{}", Uuid::new_v4()))
                .append(Element::builder(
                    "query",
                    "http://jabber.org/protocol/disco#info",
                ))
                .build();
            let Ok(Ok(result)) =
                tokio::time::timeout(std::time::Duration::from_secs(5), self.iq_request(iq)).await
            else {
                continue;
            };
            let Some(query) = result.get_child("query", "http://jabber.org/protocol/disco#info")
            else {
                continue;
            };
            let features: Vec<String> = query
                .children()
                .filter(|child| child.name() == "feature")
                .filter_map(|child| child.attr("var").map(str::to_string))
                .collect();
            let spaces_capable = features.iter().any(|f| f == crate::spaces::NS_SPACES);
            let pubsub = features
                .iter()
                .any(|f| f == "http://jabber.org/protocol/pubsub#create-nodes");
            if spaces_capable || pubsub {
                let bare = jid.to_bare().to_string();
                self.disco.set_spaces_service(Some(bare.clone()));
                self.disco.add_pubsub_service(bare);
                return Ok(Jid::from(jid.to_bare()));
            }
        }
        Err(XmppError::ProtocolError(
            "no spaces service found; set one manually or ask the server administrator".into(),
        ))
    }

    /// The currently selected spaces service, if any.
    #[must_use]
    pub fn spaces_service(&self) -> Option<String> {
        self.disco.spaces_service()
    }

    /// Override the spaces service (for servers that do not advertise it).
    pub fn set_spaces_service(&self, service: Option<&str>) {
        self.disco.set_spaces_service(service.map(str::to_string));
    }

    /// Refresh the list of spaces from the resolved spaces service.
    ///
    /// # Errors
    ///
    /// Returns an error when no service is available or the stanza cannot be sent.
    pub async fn refresh_spaces(&self) -> Result<()> {
        let service = self.resolve_spaces_service().await?;
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", service.to_string())
            .attr("id", format!("spaces-items-{}", Uuid::new_v4()))
            .append(Element::builder(
                "query",
                "http://jabber.org/protocol/disco#items",
            ))
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Fetch the metadata of one space node.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the disco request.
    pub async fn fetch_space_info(&self, id: &crate::spaces::SpaceId) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", id.server.clone())
            .attr("id", format!("space-info-{}", Uuid::new_v4()))
            .append(
                Element::builder("query", "http://jabber.org/protocol/disco#info")
                    .attr("node", id.node.clone()),
            )
            .build();
        let result = self.iq_request(iq).await?;
        let Some(query) = result.get_child("query", "http://jabber.org/protocol/disco#info") else {
            return Ok(());
        };

        let form = query
            .children()
            .find(|child| child.name() == "x" && child.ns() == "jabber:x:data");
        let title = form
            .and_then(|form| form_value(form, "pubsub#title"))
            .filter(|value| !value.is_empty());
        let description = form
            .and_then(|form| form_value(form, "pubsub#description"))
            .unwrap_or_default();
        let members = form
            .and_then(|form| form_value(form, "pubsub#num_subscribers"))
            .and_then(|value| value.parse::<usize>().ok());
        let locked = form
            .and_then(|form| form_value(form, "pubsub#access_model"))
            .is_some_and(|value| value != "open");
        self.spaces
            .set_metadata(id, title, Some(description), None, members, Some(locked));
        self.spaces.mark_known(id.clone());
        Ok(())
    }

    /// Fetch the rooms, directories and avatar of one space.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the items request.
    pub async fn fetch_space_items(&self, id: &crate::spaces::SpaceId) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", id.server.clone())
            .attr("id", format!("space-items-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("items", "http://jabber.org/protocol/pubsub")
                        .attr("node", id.node.clone()),
                ),
            )
            .build();
        let result = self.iq_request(iq).await?;
        let Some(pubsub) = result.get_child("pubsub", "http://jabber.org/protocol/pubsub") else {
            return Ok(());
        };
        let Some(items) = pubsub.get_child("items", "http://jabber.org/protocol/pubsub") else {
            return Ok(());
        };

        let mut rooms = Vec::new();
        let mut directories = Vec::new();
        let mut avatar = None;
        for item in items.children().filter(|child| child.name() == "item") {
            let item_id = item.attr("id").unwrap_or_default();
            if item_id == crate::spaces::NS_SPACE_AVATAR {
                if let Some(url) = item
                    .get_child("metadata", "urn:xmpp:avatar:metadata")
                    .and_then(|metadata| metadata.get_child("info", "urn:xmpp:avatar:metadata"))
                    .and_then(|info| info.attr("url"))
                {
                    avatar = Some(url.to_string());
                }
                continue;
            }
            if let Some(node) = item.get_child("directories", crate::spaces::NS_SPACE_DIRECTORIES) {
                directories = parse_space_directories(node);
                continue;
            }
            if let Some(conference) = item.get_child("conference", crate::spaces::NS_BOOKMARKS) {
                if let Some(room) = parse_space_room(item_id, conference) {
                    rooms.push(room);
                }
            }
        }
        rooms.sort_by_key(|room: &crate::spaces::SpaceRoom| room.weight);
        self.spaces.set_metadata(id, None, None, avatar, None, None);
        self.spaces.set_rooms(id, rooms);
        self.spaces.set_directories(id, directories);
        self.spaces.mark_known(id.clone());
        Ok(())
    }

    /// Subscribe to a space node (join the space).
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the subscription.
    pub async fn join_space(&self, id: &crate::spaces::SpaceId) -> Result<()> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", id.server.clone())
            .attr("id", format!("space-join-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("subscribe", "http://jabber.org/protocol/pubsub")
                        .attr("node", id.node.clone())
                        .attr("jid", own.to_bare().to_string()),
                ),
            )
            .build();
        match self.iq_request(iq).await {
            Ok(_) => {
                self.spaces
                    .set_subscription(id, crate::spaces::SpaceSubscription::Subscribed);
                Ok(())
            }
            Err(error) => {
                self.spaces
                    .set_subscription(id, crate::spaces::SpaceSubscription::Pending);
                Err(error)
            }
        }
    }

    /// Unsubscribe from a space node (leave the space).
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the request.
    pub async fn leave_space(&self, id: &crate::spaces::SpaceId) -> Result<()> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", id.server.clone())
            .attr("id", format!("space-leave-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("unsubscribe", "http://jabber.org/protocol/pubsub")
                        .attr("node", id.node.clone())
                        .attr("jid", own.to_bare().to_string()),
                ),
            )
            .build();
        self.iq_request(iq).await?;
        self.spaces
            .set_subscription(id, crate::spaces::SpaceSubscription::None);
        Ok(())
    }

    /// Create a space node and subscribe to it.
    ///
    /// Public spaces use an open access model and can be joined by anyone;
    /// private spaces use `authorize`, so subscriptions need approval.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects node creation.
    pub async fn create_space(
        &self,
        service: &Jid,
        title: &str,
        private: bool,
    ) -> Result<crate::spaces::SpaceId> {
        let node = Uuid::new_v4().simple().to_string();
        let access_model = if private { "authorize" } else { "open" };
        let form = Element::builder("x", "jabber:x:data")
            .attr("type", "submit")
            .append(data_form_field(
                "FORM_TYPE",
                "http://jabber.org/protocol/pubsub#node_config",
                true,
            ))
            .append(data_form_field("pubsub#title", title, false))
            .append(data_form_field(
                "pubsub#type",
                crate::spaces::NS_SPACES,
                false,
            ))
            .append(data_form_field("pubsub#notify_retract", "true", false))
            .append(data_form_field("pubsub#persist_items", "true", false))
            .append(data_form_field("pubsub#purge_offline", "false", false))
            .append(data_form_field("pubsub#notify_sub", "true", false))
            .append(data_form_field("pubsub#notify_config", "true", false))
            .append(data_form_field("pubsub#notify_delete", "true", false))
            .append(data_form_field("pubsub#max_items", "max", false))
            .append(data_form_field("pubsub#publish_model", "publishers", false))
            .append(data_form_field("pubsub#access_model", access_model, false))
            .append(data_form_field("pubsub#deliver_payloads", "true", false));
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", service.to_string())
            .attr("id", format!("space-create-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub")
                    .append(
                        Element::builder("create", "http://jabber.org/protocol/pubsub")
                            .attr("node", node.clone()),
                    )
                    .append(
                        Element::builder("configure", "http://jabber.org/protocol/pubsub")
                            .append(form),
                    ),
            )
            .build();
        self.iq_request(iq).await?;

        let id = crate::spaces::SpaceId::new(service.to_string(), node);
        self.spaces.set_metadata(
            &id,
            Some(title.to_string()),
            None,
            None,
            None,
            Some(private),
        );
        self.spaces
            .set_affiliation(&id, crate::spaces::SpaceAffiliation::Owner);
        let _ = self.join_space(&id).await;
        Ok(id)
    }

    /// Delete a space node.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the deletion.
    pub async fn destroy_space(&self, id: &crate::spaces::SpaceId) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", id.server.clone())
            .attr("id", format!("space-destroy-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub#owner").append(
                    Element::builder("delete", "http://jabber.org/protocol/pubsub#owner")
                        .attr("node", id.node.clone()),
                ),
            )
            .build();
        self.iq_request(iq).await?;
        self.spaces.remove(id);
        Ok(())
    }

    /// Publish or update one room item in a space node.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the publication.
    pub async fn save_space_room(
        &self,
        id: &crate::spaces::SpaceId,
        room: &crate::spaces::SpaceRoom,
    ) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", id.server.clone())
            .attr("id", format!("space-room-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("publish", "http://jabber.org/protocol/pubsub")
                        .attr("node", id.node.clone())
                        .append(space_room_item(room)),
                ),
            )
            .build();
        self.iq_request(iq).await?;
        self.fetch_space_items(id).await
    }

    /// Delete one room item from a space node.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the retraction.
    pub async fn delete_space_room(
        &self,
        id: &crate::spaces::SpaceId,
        conference: &str,
    ) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", id.server.clone())
            .attr("id", format!("space-room-del-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("retract", "http://jabber.org/protocol/pubsub")
                        .attr("node", id.node.clone())
                        .attr("notify", "true")
                        .append(
                            Element::builder("item", "http://jabber.org/protocol/pubsub")
                                .attr("id", conference),
                        ),
                ),
            )
            .build();
        self.iq_request(iq).await?;
        self.fetch_space_items(id).await
    }

    /// Publish the directory hierarchy item of a space.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the publication.
    pub async fn save_space_directories(
        &self,
        id: &crate::spaces::SpaceId,
        directories: &[crate::spaces::SpaceDirectory],
    ) -> Result<()> {
        let mut node = Element::builder("directories", crate::spaces::NS_SPACE_DIRECTORIES);
        for directory in directories {
            node = node.append(
                Element::builder("directory", crate::spaces::NS_SPACE_DIRECTORIES)
                    .attr("id", directory.id.clone())
                    .attr("title", directory.name.clone()),
            );
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", id.server.clone())
            .attr("id", format!("space-dirs-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("publish", "http://jabber.org/protocol/pubsub")
                        .attr("node", id.node.clone())
                        .append(
                            Element::builder("item", "http://jabber.org/protocol/pubsub")
                                .attr("id", crate::spaces::NS_SPACE_DIRECTORIES)
                                .append(node),
                        ),
                ),
            )
            .build();
        self.iq_request(iq).await?;
        self.fetch_space_items(id).await
    }

    /// Set the notification mode of a space locally.
    pub fn set_space_notify(
        &self,
        id: &crate::spaces::SpaceId,
        notify: crate::spaces::SpaceNotify,
    ) {
        self.spaces.set_notify(id, notify);
    }

    /// Pin or unpin a space locally.
    pub fn set_space_pinned(&self, id: &crate::spaces::SpaceId, pinned: bool) {
        self.spaces.set_pinned(id, pinned);
    }

    /// Set the local affiliation of the account in a space.
    pub fn set_space_affiliation(
        &self,
        id: &crate::spaces::SpaceId,
        affiliation: crate::spaces::SpaceAffiliation,
    ) {
        self.spaces.set_affiliation(id, affiliation);
    }

    /// Fetch affiliations and subscriptions for a space into one member list.
    ///
    /// # Errors
    ///
    /// Returns an error when either owner request is rejected.
    pub async fn fetch_space_members(
        &self,
        id: &crate::spaces::SpaceId,
    ) -> Result<Vec<crate::spaces::SpaceMember>> {
        let affiliations = self.fetch_space_affiliations(id).await?;
        let subscriptions = self.fetch_space_subscriptions(id).await?;
        let mut members: Vec<crate::spaces::SpaceMember> = Vec::new();
        for (jid, affiliation) in affiliations {
            members.push(crate::spaces::SpaceMember {
                jid,
                affiliation,
                subscription: crate::spaces::SpaceSubscription::None,
                sub_id: None,
            });
        }
        for (jid, subscription, sub_id) in subscriptions {
            if let Some(member) = members.iter_mut().find(|member| member.jid == jid) {
                member.subscription = subscription;
                member.sub_id = sub_id;
            } else {
                members.push(crate::spaces::SpaceMember {
                    jid,
                    affiliation: crate::spaces::SpaceAffiliation::None,
                    subscription,
                    sub_id,
                });
            }
        }
        members.sort_by(|a, b| a.jid.cmp(&b.jid));
        self.spaces.set_subscriptions(id, members.clone());
        Ok(members)
    }

    /// Fetch owner affiliations of a space.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the request.
    pub async fn fetch_space_affiliations(
        &self,
        id: &crate::spaces::SpaceId,
    ) -> Result<Vec<(String, crate::spaces::SpaceAffiliation)>> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", id.server.clone())
            .attr("id", format!("space-affiliations-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub#owner").append(
                    Element::builder("affiliations", "http://jabber.org/protocol/pubsub#owner")
                        .attr("node", id.node.clone()),
                ),
            )
            .build();
        let result = self.iq_request(iq).await?;
        let mut out = Vec::new();
        if let Some(pubsub) = result.get_child("pubsub", "http://jabber.org/protocol/pubsub#owner")
        {
            if let Some(node) =
                pubsub.get_child("affiliations", "http://jabber.org/protocol/pubsub#owner")
            {
                for item in node
                    .children()
                    .filter(|child| child.name() == "affiliation")
                {
                    if let (Some(jid), Some(affiliation)) =
                        (item.attr("jid"), item.attr("affiliation"))
                    {
                        out.push((
                            jid.to_string(),
                            crate::spaces::SpaceAffiliation::from_str(affiliation),
                        ));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Fetch owner subscriptions of a space.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the request.
    pub async fn fetch_space_subscriptions(
        &self,
        id: &crate::spaces::SpaceId,
    ) -> Result<Vec<(String, crate::spaces::SpaceSubscription, Option<String>)>> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", id.server.clone())
            .attr("id", format!("space-subscriptions-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub#owner").append(
                    Element::builder("subscriptions", "http://jabber.org/protocol/pubsub#owner")
                        .attr("node", id.node.clone()),
                ),
            )
            .build();
        let result = self.iq_request(iq).await?;
        let mut out = Vec::new();
        if let Some(pubsub) = result.get_child("pubsub", "http://jabber.org/protocol/pubsub#owner")
        {
            if let Some(node) =
                pubsub.get_child("subscriptions", "http://jabber.org/protocol/pubsub#owner")
            {
                for item in node
                    .children()
                    .filter(|child| child.name() == "subscription")
                {
                    if let Some(jid) = item.attr("jid") {
                        let subscription = match item.attr("subscription") {
                            Some("subscribed") => crate::spaces::SpaceSubscription::Subscribed,
                            Some("pending") => crate::spaces::SpaceSubscription::Pending,
                            _ => crate::spaces::SpaceSubscription::None,
                        };
                        out.push((
                            jid.to_string(),
                            subscription,
                            item.attr("subid").map(str::to_string),
                        ));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Set a member subscription state in a space.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the change.
    pub async fn set_space_subscription(
        &self,
        id: &crate::spaces::SpaceId,
        jid: &str,
        state: crate::spaces::SpaceSubscription,
    ) -> Result<()> {
        let state = match state {
            crate::spaces::SpaceSubscription::Subscribed => "subscribed",
            crate::spaces::SpaceSubscription::Pending => "pending",
            crate::spaces::SpaceSubscription::None => "none",
        };
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", id.server.clone())
            .attr("id", format!("space-sub-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub#owner").append(
                    Element::builder("subscriptions", "http://jabber.org/protocol/pubsub#owner")
                        .attr("node", id.node.clone())
                        .append(
                            Element::builder(
                                "subscription",
                                "http://jabber.org/protocol/pubsub#owner",
                            )
                            .attr("jid", jid.to_string())
                            .attr("subscription", state),
                        ),
                ),
            )
            .build();
        self.iq_request(iq).await?;
        let _ = self.fetch_space_members(id).await;
        Ok(())
    }

    /// Set affiliations in a space.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the change.
    pub async fn set_space_affiliations(
        &self,
        id: &crate::spaces::SpaceId,
        members: &[(&str, crate::spaces::SpaceAffiliation)],
    ) -> Result<()> {
        let mut affiliations =
            Element::builder("affiliations", "http://jabber.org/protocol/pubsub#owner")
                .attr("node", id.node.clone());
        for (jid, affiliation) in members {
            affiliations = affiliations.append(
                Element::builder("affiliation", "http://jabber.org/protocol/pubsub#owner")
                    .attr("jid", (*jid).to_string())
                    .attr("affiliation", affiliation_slug(*affiliation)),
            );
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", id.server.clone())
            .attr("id", format!("space-aff-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub#owner")
                    .append(affiliations),
            )
            .build();
        self.iq_request(iq).await?;
        let _ = self.fetch_space_members(id).await;
        Ok(())
    }

    /// Invite a JID into a space and grant it a member affiliation.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the invitation.
    pub async fn invite_space_member(
        &self,
        id: &crate::spaces::SpaceId,
        jid: &str,
        affiliation: crate::spaces::SpaceAffiliation,
    ) -> Result<()> {
        self.set_space_affiliations(id, &[(jid, affiliation)])
            .await?;
        self.set_space_subscription(id, jid, crate::spaces::SpaceSubscription::Subscribed)
            .await
    }

    /// Fetch the owner configuration form of a space.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the request.
    pub async fn space_config(
        &self,
        id: &crate::spaces::SpaceId,
    ) -> Result<crate::data_form::DataForm> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", id.server.clone())
            .attr("id", format!("space-config-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub#owner").append(
                    Element::builder("configure", "http://jabber.org/protocol/pubsub#owner")
                        .attr("node", id.node.clone()),
                ),
            )
            .build();
        let result = self.iq_request(iq).await?;
        if let Some(pubsub) = result.get_child("pubsub", "http://jabber.org/protocol/pubsub#owner")
        {
            if let Some(configure) =
                pubsub.get_child("configure", "http://jabber.org/protocol/pubsub#owner")
            {
                if let Some(form) = configure.get_child("x", "jabber:x:data") {
                    return Ok(crate::data_form::parse(form));
                }
            }
        }
        Err(XmppError::ProtocolError(
            "The service did not return a space configuration form".to_string(),
        ))
    }

    /// Submit a space owner configuration form.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the configuration.
    pub async fn submit_space_config(
        &self,
        id: &crate::spaces::SpaceId,
        form: Option<&Element>,
        values: &[(String, String)],
    ) -> Result<()> {
        let submit = crate::data_form::submit(form, values);
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", id.server.clone())
            .attr("id", format!("space-config-set-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub#owner").append(
                    Element::builder("configure", "http://jabber.org/protocol/pubsub#owner")
                        .attr("node", id.node.clone())
                        .append(submit),
                ),
            )
            .build();
        self.iq_request(iq).await?;
        let _ = self.fetch_space_info(id).await;
        Ok(())
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

    #[test]
    fn space_room_item_roundtrips() {
        let room = crate::spaces::SpaceRoom {
            conference: "dev@conference.example.org".to_string(),
            name: "development".to_string(),
            nick: Some("alice".to_string()),
            autojoin: true,
            directory_id: Some("dev".to_string()),
            notify: crate::spaces::SpaceNotify::Mentions,
            pinned: true,
            weight: 2,
        };
        let item = space_room_item(&room);
        let conference = item
            .get_child("conference", crate::spaces::NS_BOOKMARKS)
            .expect("conference");
        let parsed = parse_space_room("dev@conference.example.org", conference).expect("room");
        assert_eq!(parsed.name, "development");
        assert!(parsed.autojoin);
        assert_eq!(parsed.directory_id.as_deref(), Some("dev"));
        assert_eq!(parsed.notify, crate::spaces::SpaceNotify::Mentions);
        assert!(parsed.pinned);
        assert_eq!(parsed.weight, 2);
    }
}
