//! XMPP Core - Direct tokio-xmpp implementation for Jabu

pub mod adhoc;
pub mod avatar;
pub mod bob;
pub mod caps;
pub mod change;
pub mod channel_search;
pub mod chat;
pub mod color;
pub mod conference;
pub mod connection;
pub mod data_form;
pub mod disco;
pub mod error;
pub mod events;
pub mod gateway;
pub mod iq;
pub mod jingle;
pub mod link_preview;
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
pub mod voice;

pub use adhoc::{AdHocActions, AdHocCommand, AdHocNote, AdHocResponse, AdHocStatus};
pub use bob::{BobContent, BobRequest};
pub use caps::{caps_element, verification as caps_verification};
pub use channel_search::{ChannelSearchItem, ChannelSearchPage};
pub use chat::{
    Chat, ChatManager, ChatMessage, ChatPreferences, ChatState, MessageReceipt, MessageType,
};
pub use color::identity_color;
pub use conference::{
    ConferenceCategory, ConferenceContent, ConferenceDialect, ConferenceEvent, ConferenceInfo,
    ConferenceJingle, ConferenceManager, ConferenceSession, ConferenceState, ConferenceUpdate,
    ConferenceUser, SfuDiscovery,
};
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
pub use link_preview::LinkPreview;
pub use mam::{MamManager, MamQuery};
pub use microblog::{build_entry, parse_item, StoryRef, MICROBLOG_NODE, STORIES_NODE};
pub use muc::{MucAffiliation, MucConfig, MucManager, MucMessage, MucOccupant, MucRole, MucRoom};
pub use omemo::{
    DeviceBundle, LocalDevice, OmemoEnvelope, OmemoKey, OmemoManager, OmemoStore, PreKeyPair,
    PreKeyPublic, TrustLevel,
};
pub use pep::{activity, location, mood, tune};
pub use presence::presence_element;
pub use processor::{EventProcessor, ProcessorNotifier};
pub use pubsub::{
    CommentRef, PubSubItem, PubSubManager, PubSubNode, PubSubService, PubSubSubscription,
};
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
pub use voice::{VoiceChannel, VoiceEvent, VoiceJoin, VoiceManager, VoiceParticipant, VoicePort};

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
    pub conference: Arc<crate::conference::ConferenceManager>,
    pub spaces: Arc<SpaceManager>,
    pub voice: Arc<VoiceManager>,
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
    let mut kind = crate::spaces::SpaceRoomKind::Text;
    if let Some(extensions) = conference.get_child("extensions", minidom::NSChoice::Any) {
        if extensions
            .get_child("voice", crate::spaces::NS_SPACE_VOICE)
            .is_some()
        {
            kind = crate::spaces::SpaceRoomKind::Voice;
        }
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
        kind,
        nick,
        autojoin,
        directory_id,
        notify,
        pinned,
        weight,
    })
}

/// Build a space avatar/banner item with XEP-0084 metadata (XEP-0503 §Images).
fn space_image_item(
    item_id: &str,
    url: &str,
    mime: &str,
    bytes: &[u8],
    width: Option<u32>,
    height: Option<u32>,
) -> Element {
    use sha1::{Digest, Sha1};
    let digest = Sha1::digest(bytes);
    let mut info = Element::builder("info", "urn:xmpp:avatar:metadata")
        .attr("id", format!("{digest:x}"))
        .attr("url", url)
        .attr("type", mime)
        .attr("bytes", bytes.len().to_string());
    if let Some(width) = width {
        info = info.attr("width", width.to_string());
    }
    if let Some(height) = height {
        info = info.attr("height", height.to_string());
    }
    Element::builder("item", "http://jabber.org/protocol/pubsub")
        .attr("id", item_id)
        .append(Element::builder("metadata", "urn:xmpp:avatar:metadata").append(info))
        .build()
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
    if room.kind == crate::spaces::SpaceRoomKind::Voice {
        extensions = extensions.append(Element::builder("voice", crate::spaces::NS_SPACE_VOICE));
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

    /// Send a 1:1 message carrying an XEP-0501 story reference.
    ///
    /// The body is the optional comment or share text; the reference lets a
    /// Movim-compatible peer render the story card. An OOB fallback is added
    /// for clients that only read `jabber:x:oob`.
    ///
    /// # Errors
    ///
    /// Returns an error when the client is not connected or the stanza cannot
    /// be queued.
    pub fn send_story_message(
        &self,
        to: &Jid,
        body: &str,
        reference: &StoryRef,
    ) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();
        let body = body.trim();
        let body = if body.is_empty() {
            microblog::STORY_SHARE_BODY
        } else {
            body
        };
        let uri = microblog::story_href(&reference.service, &reference.item_id);
        let stanza = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", id.clone())
            .append(Element::builder("body", ns::JABBER_CLIENT).append(body.to_string()))
            .append(
                Element::builder("reference", "urn:xmpp:reference:0")
                    .attr("type", "data")
                    .attr("uri", uri.clone()),
            )
            .append(
                Element::builder("x", "jabber:x:oob")
                    .append(Element::builder("url", "jabber:x:oob").append(uri)),
            )
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
        msg.story_ref = Some(reference.clone());

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
        self.jingle.set_conference(self.conference.clone());
        self.jingle.start_media_listener();
    }

    /// Start an outgoing audio/video call (XEP-0353 propose).
    pub async fn start_call(&self, to: &Jid, kind: crate::jingle::CallKind) -> Result<String> {
        self.jingle.start_call(to, kind).await
    }

    /// Start the voice conference of a room that has a discovered SFU.
    ///
    /// This sends the `propose` and returns the new session. The media backend
    /// follows up with `session-initiate` once it has an offer.
    pub fn start_voice_conference(
        &self,
        room: &Jid,
    ) -> Result<crate::conference::ConferenceSession> {
        let discovery = self
            .conference
            .discovery(room)
            .ok_or_else(|| XmppError::ProtocolError("no SFU discovered in this room".into()))?;
        let session = self.conference.begin(&discovery);
        let propose = crate::conference::propose_message(&session.sfu, &session.sid, false);
        self.connection.send_stanza_now(propose)?;
        Ok(session)
    }

    /// Start sharing the screen in a voice room (Galener's second session).
    ///
    /// # Errors
    ///
    /// Returns an error when there is no active voice conference or the media
    /// backend cannot create a screen offer.
    pub async fn start_voice_screen(&self, room: &Jid) -> Result<()> {
        if self.conference.screen_session(room).is_some() {
            return Ok(());
        }
        let primary = self
            .conference
            .session(room)
            .ok_or_else(|| XmppError::ProtocolError("not in this voice conference".into()))?;
        // The capture pipeline is built first: it is the step that fails on a
        // desktop with no portal or no capture plugin, and a session registered
        // before it would leave the UI showing a share that does not exist.
        let sid = uuid::Uuid::new_v4().to_string();
        let offer = self.jingle.create_conference_screen_offer(&sid).await?;
        let offer = offer
            .parse::<Element>()
            .map_err(|error| XmppError::ProtocolError(error.to_string()))?;
        let screen = self.conference.begin_screen_with_sid(&primary, &sid);
        let own = self.connection.jid().unwrap_or_else(|| screen.sfu.clone());
        let iq = crate::conference::session_initiate_with_offer(
            &screen.sfu,
            &screen.sid,
            &own,
            offer,
            screen.dialect,
        );
        if let Err(error) = self.connection.send_stanza_now(iq) {
            self.conference.remove_session(&sid);
            return Err(error);
        }
        Ok(())
    }

    /// Stop the voice-room screen share.
    ///
    /// # Errors
    ///
    /// Returns an error when the terminate stanza cannot be sent.
    pub async fn stop_voice_screen(&self, room: &Jid) -> Result<()> {
        let Some(screen) = self.conference.screen_session(room) else {
            return Ok(());
        };
        let terminate =
            crate::conference::session_terminate_iq(&screen.sfu, &screen.sid, "success");
        self.connection.send_stanza_now(terminate)?;
        self.jingle.stop_conference_screen(&screen.sid);
        self.conference.remove_session(&screen.sid);
        Ok(())
    }

    /// Leave the voice conference of a room.
    pub fn leave_voice_conference(&self, room: &Jid) -> Result<()> {
        for session in self.conference.sessions_for(room) {
            let terminate =
                crate::conference::session_terminate_iq(&session.sfu, &session.sid, "success");
            let _ = self.connection.send_stanza_now(terminate);
            if session.screen {
                self.jingle.stop_conference_screen(&session.sid);
            } else {
                self.jingle.stop_session_media(&session.sid);
            }
        }
        self.conference.remove(room);
        Ok(())
    }

    /// Send a media-produced Jingle offer as the conference `session-initiate`.
    pub fn send_conference_offer(&self, room: &Jid, payload: Element) -> Result<()> {
        let session = self.conference.session(room).ok_or_else(|| {
            XmppError::ProtocolError("no conference session for this room".into())
        })?;
        let own = self.connection.jid().unwrap_or_else(|| session.sfu.clone());
        let iq = crate::conference::session_initiate_iq(&session.sfu, &session.sid, &own, payload);
        self.connection.send_stanza_now(iq)?;
        Ok(())
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
        self.run_command(jid, node, None, "execute", form).await
    }

    /// Send one XEP-0050 command action (`execute`, `next`, `prev`,
    /// `complete` or `cancel`) and parse the next stage.
    ///
    /// # Errors
    ///
    /// Returns an error when the command is rejected or the reply is malformed.
    pub async fn run_command(
        &self,
        jid: &Jid,
        node: &str,
        sessionid: Option<&str>,
        action: &str,
        payload: Option<Element>,
    ) -> Result<AdHocResponse> {
        let iq = crate::adhoc::command_iq(jid, node, sessionid, action, payload);
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
            self.disco
                .set_service_features(&jid.to_bare().to_string(), features.clone());
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
        let filtered = self.disco.has_service_feature(
            &service.to_string(),
            crate::pubsub::discovery::NS_PUBSUB_FILTER,
        );
        let iq = spaces_items_iq(&service, filtered)?;
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
        let mut banner = None;
        for item in items.children().filter(|child| child.name() == "item") {
            let item_id = item.attr("id").unwrap_or_default();
            if item_id == crate::spaces::NS_SPACE_AVATAR
                || item_id == crate::spaces::NS_SPACE_BANNER
            {
                let url = item
                    .get_child("metadata", "urn:xmpp:avatar:metadata")
                    .and_then(|metadata| metadata.get_child("info", "urn:xmpp:avatar:metadata"))
                    .and_then(|info| info.attr("url"))
                    .map(str::to_string);
                if item_id == crate::spaces::NS_SPACE_AVATAR {
                    avatar = url;
                } else {
                    banner = url;
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
        self.spaces.set_banner(id, banner);
        self.spaces.set_rooms(id, rooms);
        self.spaces.set_directories(id, directories);
        self.spaces.mark_known(id.clone());
        Ok(())
    }

    /// Re-read the room list of every known space.
    ///
    /// A space's rooms live in a pubsub node and the server only announces
    /// changes made while the client is online, so a client that was away when
    /// a room was published would keep a stale list for good. Ask again on
    /// every connection. Subscribing is idempotent, which also repairs a space
    /// that was added without one.
    pub async fn refresh_space_rooms(&self) {
        for space in self.spaces.list() {
            let id = space.id.clone();
            if space.subscription != crate::spaces::SpaceSubscription::Subscribed {
                if let Err(error) = self.join_space(&id).await {
                    tracing::debug!(node = %id.node, "space subscribe failed: {error}");
                }
            }
            if let Err(error) = self.fetch_space_items(&id).await {
                tracing::debug!(node = %id.node, "space items fetch failed: {error}");
            }
        }
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
                let (title, notify, pinned) = self
                    .spaces
                    .get(id)
                    .map(|space| (space.name, space.notify, space.pinned))
                    .unwrap_or_default();
                if let Err(error) = self
                    .publish_subscription(
                        &id.server,
                        &id.node,
                        Some(title.as_str()),
                        Some(notify.slug()),
                        pinned,
                    )
                    .await
                {
                    tracing::debug!(node = %id.node, "public subscription publish failed: {error}");
                }
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
        if let Err(error) = self.retract_subscription(&id.server, &id.node).await {
            tracing::debug!(node = %id.node, "public subscription retract failed: {error}");
        }
        Ok(())
    }

    /// Publish or update our entry in the XEP-0330 public subscription list.
    ///
    /// # Errors
    ///
    /// Returns an error when the client is not connected or the server rejects
    /// the publication.
    pub async fn publish_subscription(
        &self,
        server: &str,
        node: &str,
        title: Option<&str>,
        notify: Option<&str>,
        pinned: bool,
    ) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        let entry = crate::pubsub::subscriptions::SubscriptionEntry {
            server: server.to_string(),
            node: node.to_string(),
            title: title.map(str::to_string),
            notify: notify.map(str::to_string),
            pinned,
        };
        let iq = crate::pubsub::subscriptions::publish_iq(&own, &entry);
        self.iq_request(iq).await?;
        Ok(())
    }

    /// Remove our entry from the XEP-0330 public subscription list.
    ///
    /// # Errors
    ///
    /// Returns an error when the client is not connected or the server rejects
    /// the retraction.
    pub async fn retract_subscription(&self, server: &str, node: &str) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        let iq = crate::pubsub::subscriptions::retract_iq(&own, server, node);
        self.iq_request(iq).await?;
        Ok(())
    }

    /// Fetch another account's XEP-0330 public subscription list.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails or the reply is malformed.
    pub async fn fetch_subscriptions(
        &self,
        jid: &Jid,
    ) -> Result<Vec<crate::pubsub::subscriptions::SubscriptionEntry>> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", jid.to_bare().to_string())
            .attr("id", format!("subscriptions-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("items", "http://jabber.org/protocol/pubsub")
                        .attr("node", crate::pubsub::subscriptions::NS_SUBSCRIPTION),
                ),
            )
            .build();
        let result = self.iq_request(iq).await?;
        let pubsub = result
            .get_child("pubsub", "http://jabber.org/protocol/pubsub")
            .ok_or_else(|| XmppError::ProtocolError("no pubsub result returned".into()))?;
        let items = pubsub
            .get_child("items", "http://jabber.org/protocol/pubsub")
            .ok_or_else(|| XmppError::ProtocolError("no subscription items returned".into()))?;
        Ok(crate::pubsub::subscriptions::parse_items(items))
    }

    /// Subscribe to a node with XEP-0497 hierarchy/link options.
    ///
    /// # Errors
    ///
    /// Returns an error when the client is not connected or the service
    /// rejects the subscription.
    pub async fn subscribe_with_options(
        &self,
        service: &Jid,
        node: &str,
        options: &crate::pubsub::extended_subscriptions::ExtendedSubscription,
    ) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        let iq = crate::pubsub::extended_subscriptions::subscribe_iq(service, node, &own, options);
        self.iq_request(iq).await?;
        Ok(())
    }

    /// Run a XEP-0499 extended discovery query against a `PubSub` service.
    ///
    /// # Errors
    ///
    /// Returns an error when the request is invalid or the service rejects it.
    pub async fn discover_pubsub_nodes(
        &self,
        service: &Jid,
        request: &crate::pubsub::discovery::ExtendedDiscovery,
    ) -> Result<Vec<crate::pubsub::discovery::DiscoveredNode>> {
        let query = crate::pubsub::discovery::extended_items_query(request)?;
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", service.to_string())
            .attr("id", format!("pubsub-disco-{}", Uuid::new_v4()))
            .append(query)
            .build();
        let result = self.iq_request(iq).await?;
        let query = result
            .get_child("query", crate::pubsub::discovery::NS_DISCO_ITEMS)
            .ok_or_else(|| XmppError::ProtocolError("no disco items returned".into()))?;
        Ok(crate::pubsub::discovery::parse_extended_items(query))
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
        if let Err(error) = self.retract_subscription(&id.server, &id.node).await {
            tracing::debug!(node = %id.node, "public subscription retract failed: {error}");
        }
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

    /// Publish a space avatar as XEP-0084 metadata on the space node.
    ///
    /// The image itself is uploaded first (HTTP Upload); this writes the
    /// XEP-0503 item that points at it.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the publication.
    pub async fn publish_space_avatar(
        &self,
        id: &crate::spaces::SpaceId,
        url: &str,
        mime: &str,
        bytes: &[u8],
        width: Option<u32>,
        height: Option<u32>,
    ) -> Result<()> {
        self.publish_space_image(
            id,
            crate::spaces::NS_SPACE_AVATAR,
            url,
            mime,
            bytes,
            width,
            height,
        )
        .await
    }

    /// Publish a space banner, using the same XEP-0084 metadata format.
    ///
    /// # Errors
    ///
    /// Returns an error when the service rejects the publication.
    pub async fn publish_space_banner(
        &self,
        id: &crate::spaces::SpaceId,
        url: &str,
        mime: &str,
        bytes: &[u8],
        width: Option<u32>,
        height: Option<u32>,
    ) -> Result<()> {
        self.publish_space_image(
            id,
            crate::spaces::NS_SPACE_BANNER,
            url,
            mime,
            bytes,
            width,
            height,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn publish_space_image(
        &self,
        id: &crate::spaces::SpaceId,
        item_id: &str,
        url: &str,
        mime: &str,
        bytes: &[u8],
        width: Option<u32>,
        height: Option<u32>,
    ) -> Result<()> {
        if url.trim().is_empty() {
            return Err(XmppError::ProtocolError(
                "a space image needs an uploaded URL".into(),
            ));
        }
        let item = space_image_item(item_id, url, mime, bytes, width, height);
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", id.server.clone())
            .attr("id", format!("space-image-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("publish", "http://jabber.org/protocol/pubsub")
                        .attr("node", id.node.clone())
                        .append(item),
                ),
            )
            .build();
        self.iq_request(iq).await?;
        if item_id == crate::spaces::NS_SPACE_AVATAR {
            self.spaces
                .set_metadata(id, None, None, Some(url.to_string()), None, None);
        } else {
            self.spaces.set_banner(id, Some(url.to_string()));
        }
        Ok(())
    }

    /// Set the notification mode of a space locally.
    pub fn set_space_notify(
        &self,
        id: &crate::spaces::SpaceId,
        notify: crate::spaces::SpaceNotify,
    ) {
        self.spaces.set_notify(id, notify);
        self.queue_subscription_update(id);
    }

    /// Pin or unpin a space locally.
    pub fn set_space_pinned(&self, id: &crate::spaces::SpaceId, pinned: bool) {
        self.spaces.set_pinned(id, pinned);
        self.queue_subscription_update(id);
    }

    /// Best-effort refresh of the public subscription entry after a local
    /// notify/pin change. Joining already awaits the initial publication.
    fn queue_subscription_update(&self, id: &crate::spaces::SpaceId) {
        let Some(own) = self.connection.jid().map(|jid| jid.to_bare()) else {
            return;
        };
        let Some(space) = self.spaces.get(id) else {
            return;
        };
        if space.subscription != crate::spaces::SpaceSubscription::Subscribed {
            return;
        }
        let entry = crate::pubsub::subscriptions::SubscriptionEntry {
            server: id.server.clone(),
            node: id.node.clone(),
            title: Some(space.name),
            notify: Some(space.notify.slug().to_string()),
            pinned: space.pinned,
        };
        let iq = crate::pubsub::subscriptions::publish_iq(&own, &entry);
        if let Err(error) = self.connection.send_stanza_now(iq) {
            tracing::debug!(node = %id.node, "public subscription update failed: {error}");
        }
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
        self.send_muc_message_with_reply(room, nick, body, None, None)
    }

    /// Send a group chat message that replies to `reply_to` in an optional
    /// XEP-0201 thread.
    pub fn send_muc_message_with_reply(
        &self,
        room: &Jid,
        nick: &str,
        body: &str,
        reply_to: Option<&str>,
        thread: Option<&str>,
    ) -> Result<MucMessage> {
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
        if let Some(reply_to) = reply_to {
            stanza = stanza.append(
                Element::builder("reply", "urn:xmpp:reply:0")
                    .attr("id", reply_to.to_string())
                    .attr("to", room.to_bare().to_string()),
            );
        }
        if let Some(thread) = thread {
            stanza = stanza.append(Element::builder("thread", ns::JABBER_CLIENT).append(thread));
        }
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
        msg.reply_to = reply_to.map(str::to_string);
        msg.thread = thread.map(str::to_string);

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
            mime: Some(mime.to_string()),
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
            mime: Some(mime.to_string()),
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
        // Fetch both protocol generations; a peer commonly publishes only one.
        let _ = self.fetch_omemo_device_list(jid);
        let _ = self.fetch_legacy_omemo_device_list(jid);
        self.fetch_omemo_bundles(jid)
    }

    /// Bundles for the devices currently known for a contact.
    pub fn omemo_bundles(&self, jid: &Jid) -> Vec<DeviceBundle> {
        let bare = jid.to_bare().to_string();
        self.omemo
            .device_list(&bare)
            .into_iter()
            .filter_map(|id| self.omemo.bundle(&bare, id))
            .filter(|bundle| bundle.verify())
            .collect()
    }

    /// Legacy OMEMO 0.3 bundles for the devices currently known.
    pub fn omemo_legacy_bundles(&self, jid: &Jid) -> Vec<crate::omemo::LegacyDeviceBundle> {
        let bare = jid.to_bare().to_string();
        self.omemo.legacy_bundles(&bare)
    }

    /// True when the contact has at least one usable OMEMO 2 bundle.
    pub fn supports_omemo2(&self, jid: &Jid) -> bool {
        !self.omemo_bundles(jid).is_empty()
    }

    /// True when the contact has at least one usable legacy OMEMO 0.3 bundle.
    pub fn supports_legacy_omemo(&self, jid: &Jid) -> bool {
        !self.omemo_legacy_bundles(jid).is_empty()
    }

    /// Fetch OMEMO device lists and bundles for every known room member.
    pub fn fetch_muc_omemo_keys(&self, room: &Jid) -> Result<()> {
        let Some(room) = self.mucs.get(room) else {
            return Err(XmppError::ProtocolError("room is not known".into()));
        };
        let mut seen = std::collections::HashSet::new();
        for occupant in room.occupants() {
            if let Some(jid) = occupant.jid {
                let bare = jid.to_bare().to_string();
                if seen.insert(bare) {
                    let _ = self.fetch_omemo(&jid);
                }
            }
        }
        for jid in room.affiliation_jids() {
            let bare = jid.to_bare().to_string();
            if seen.insert(bare) {
                let _ = self.fetch_omemo(&jid);
            }
        }
        if let Some(own) = self.connection.jid() {
            let own = own.to_bare();
            if seen.insert(own.to_string()) {
                let _ = self.fetch_omemo(&Jid::from(own));
            }
        }
        Ok(())
    }

    /// Send an encrypted group-chat message to every room member with keys.
    ///
    /// Rooms must expose real JIDs (non-anonymous), as required by XEP-0384.
    /// Returns an error naming present members that have no usable key in
    /// either OMEMO stack; the caller can then opt out and send plaintext.
    ///
    /// # Errors
    ///
    /// Returns an error when the room is unknown, a present member has no key,
    /// no local device exists, or the stanza cannot be sent.
    pub fn send_muc_omemo_message(&self, room: &Jid, nick: &str, body: &str) -> Result<MucMessage> {
        self.ensure_muc_omemo_ready(room)?;
        let (omemo2, legacy) = self.muc_omemo_recipients(room);
        let room_bare = room.to_bare().to_string();
        let (element, namespace) = if omemo2.len() > 1 {
            let envelope = self
                .omemo
                .encrypt_group_message(body, &room_bare, &omemo2)?;
            (envelope.to_element(), crate::omemo::NS_OMEMO)
        } else if legacy.len() > 1 {
            let envelope = self.omemo.encrypt_legacy_group_message(body, &legacy)?;
            (envelope.to_element()?, crate::omemo::NS_LEGACY_OMEMO)
        } else {
            return Err(XmppError::CryptoError(
                "no room members have OMEMO keys".into(),
            ));
        };

        let id = self.send_muc_omemo_element(
            room,
            element,
            namespace,
            (namespace == crate::omemo::NS_LEGACY_OMEMO)
                .then_some("This message is OMEMO encrypted."),
        )?;
        let mut msg = MucMessage::new(
            room_bare,
            nick.to_string(),
            self.connection.jid(),
            true,
            Some(body.to_string()),
        );
        msg.id = id.clone();
        msg.origin_id = Some(id);
        msg.hints.store = true;
        msg.hints.markable = true;
        msg.encryption = Some(EncryptionInfo {
            namespace: namespace.to_string(),
            name: "OMEMO".to_string(),
            fingerprint: None,
        });
        let mut room_ref = self.mucs.get_or_create(room);
        room_ref.add_message(msg.clone());
        Ok(msg)
    }

    /// Send a key-only OMEMO heartbeat to a room.
    ///
    /// This establishes or refreshes sessions with present members without
    /// posting a visible message.
    ///
    /// # Errors
    ///
    /// Returns an error when a present member has no key or the stanza cannot
    /// be sent.
    pub fn send_muc_omemo_heartbeat(&self, room: &Jid) -> Result<()> {
        self.ensure_muc_omemo_ready(room)?;
        let (omemo2, legacy) = self.muc_omemo_recipients(room);
        let (element, namespace) = if omemo2.len() > 1 {
            (
                self.omemo
                    .encrypt_empty_group_message(&omemo2)?
                    .to_element(),
                crate::omemo::NS_OMEMO,
            )
        } else if legacy.len() > 1 {
            (
                self.omemo
                    .encrypt_legacy_empty_message(&legacy)?
                    .to_element()?,
                crate::omemo::NS_LEGACY_OMEMO,
            )
        } else {
            return Err(XmppError::CryptoError(
                "no room members have OMEMO keys".into(),
            ));
        };
        self.send_muc_omemo_element(room, element, namespace, None)?;
        Ok(())
    }

    /// Present occupants without a usable OMEMO key in either stack.
    ///
    /// The returned names are nicknames, or `nick (no real JID)` for anonymous
    /// occupants. Empty means the room is safe to encrypt for.
    #[must_use]
    pub fn muc_omemo_missing(&self, room: &Jid) -> Vec<String> {
        let Some(room_ref) = self.mucs.get(room) else {
            return Vec::new();
        };
        let own = self.connection.jid().map(|jid| jid.to_bare());
        let mut missing = Vec::new();
        for occupant in room_ref.occupants() {
            let Some(jid) = occupant.jid else {
                missing.push(format!("{} (no real JID)", occupant.nick));
                continue;
            };
            if own.as_ref().is_some_and(|own| jid.to_bare() == *own) {
                continue;
            }
            if self.omemo_bundles(&jid).is_empty() && self.omemo_legacy_bundles(&jid).is_empty() {
                missing.push(occupant.nick.clone());
            }
        }
        missing
    }

    fn ensure_muc_omemo_ready(&self, room: &Jid) -> Result<()> {
        let missing = self.muc_omemo_missing(room);
        if missing.is_empty() {
            return Ok(());
        }
        Err(XmppError::CryptoError(format!(
            "no OMEMO keys for room members: {}",
            missing.join(", ")
        )))
    }

    fn muc_omemo_members(&self, room: &Jid) -> Vec<(String, Jid)> {
        let Some(room_ref) = self.mucs.get(room) else {
            return Vec::new();
        };
        let own = self.connection.jid().map(|jid| jid.to_bare());
        let own_bare = own.as_ref().map(ToString::to_string).unwrap_or_default();
        let mut members = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for occupant in room_ref.occupants() {
            if let Some(jid) = occupant.jid {
                let bare = jid.to_bare().to_string();
                if bare != own_bare && seen.insert(bare.clone()) {
                    members.push((bare, jid));
                }
            }
        }
        for jid in room_ref.affiliation_jids() {
            let bare = jid.to_bare().to_string();
            if bare != own_bare && seen.insert(bare.clone()) {
                members.push((bare, jid));
            }
        }
        members
    }

    fn muc_omemo_recipients(
        &self,
        room: &Jid,
    ) -> (
        Vec<(String, Vec<DeviceBundle>)>,
        Vec<(String, Vec<crate::omemo::LegacyDeviceBundle>)>,
    ) {
        let mut omemo2 = Vec::new();
        let mut legacy = Vec::new();
        for (bare, jid) in self.muc_omemo_members(room) {
            let bundles = self.omemo_bundles(&jid);
            if !bundles.is_empty() {
                omemo2.push((bare.clone(), bundles));
            }
            let bundles = self.omemo_legacy_bundles(&jid);
            if !bundles.is_empty() {
                legacy.push((bare, bundles));
            }
        }
        if let Some(local) = self.omemo.local() {
            let own_jid = self.connection.jid();
            let own_bare = own_jid
                .as_ref()
                .map(|jid| jid.to_bare().to_string())
                .unwrap_or_default();
            let mut bundles = own_jid
                .as_ref()
                .map(|jid| self.omemo_bundles(jid))
                .unwrap_or_default();
            if !bundles
                .iter()
                .any(|bundle| bundle.device_id == local.device_id)
            {
                bundles.push(local.bundle());
            }
            omemo2.push((own_bare.clone(), bundles));
            legacy.push((own_bare, vec![local.legacy_bundle()]));
        }
        (omemo2, legacy)
    }

    fn send_muc_omemo_element(
        &self,
        room: &Jid,
        encrypted: Element,
        namespace: &str,
        fallback_body: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let mut message = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "groupchat")
            .attr("to", room.to_string())
            .attr("id", id.clone())
            .append(encrypted);
        if let Some(body) = fallback_body {
            message = message
                .append(Element::builder("body", ns::JABBER_CLIENT).append(body.to_string()));
        }
        let stanza = message
            .append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id.clone()))
            .append(Element::builder("markable", "urn:xmpp:chat-markers:0"))
            .append(Element::builder("store", "urn:xmpp:hints"))
            .append(
                Element::builder("encryption", "urn:xmpp:eme:0")
                    .attr("namespace", namespace)
                    .attr("name", "OMEMO"),
            )
            .build();
        self.connection.send_stanza_now(stanza)?;
        Ok(id)
    }

    /// Encrypt and send an OMEMO message (XEP-0384) to every known device.
    ///
    /// OMEMO 2 is preferred when the contact publishes a usable bundle;
    /// otherwise the client falls back to legacy OMEMO 0.3.
    pub fn send_omemo_message(&self, to: &Jid, body: &str) -> Result<ChatMessage> {
        let mut bundles = self.omemo_bundles(to);
        if !bundles.is_empty() {
            if let Some(local) = self.omemo.local() {
                if !bundles.iter().any(|b| b.device_id == local.device_id) {
                    bundles.push(local.bundle());
                }
            }
            let envelope = self
                .omemo
                .encrypt_message(&to.to_bare().to_string(), body, &bundles)?;
            return self.send_omemo_envelope(to, body, envelope.to_element(), omemo::NS_OMEMO);
        }
        if self.supports_legacy_omemo(to) {
            return self.send_legacy_omemo_message(to, body);
        }
        Err(XmppError::CryptoError(
            "no OMEMO bundles available for recipient".into(),
        ))
    }

    /// Encrypt and send using legacy OMEMO 0.3 regardless of OMEMO 2 support.
    ///
    /// # Errors
    ///
    /// Returns an error when there is no usable legacy bundle or the stanza
    /// cannot be sent.
    pub fn send_legacy_omemo_message(&self, to: &Jid, body: &str) -> Result<ChatMessage> {
        let mut bundles = self.omemo_legacy_bundles(to);
        if let Some(local) = self.omemo.local() {
            if !bundles.iter().any(|b| b.device_id == local.device_id) {
                bundles.push(local.legacy_bundle());
            }
        }
        if bundles.is_empty() {
            return Err(XmppError::CryptoError(
                "no legacy OMEMO bundles available for recipient".into(),
            ));
        }
        let envelope =
            self.omemo
                .encrypt_legacy_message(&to.to_bare().to_string(), body, &bundles)?;
        self.send_omemo_envelope(to, body, envelope.to_element()?, omemo::NS_LEGACY_OMEMO)
    }

    fn send_omemo_envelope(
        &self,
        to: &Jid,
        body: &str,
        encrypted: Element,
        namespace: &str,
    ) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();
        let mut message = Element::builder("message", ns::JABBER_CLIENT)
            .attr("type", "chat")
            .attr("to", to.to_string())
            .attr("id", id.clone())
            .append(encrypted);
        if namespace == crate::omemo::NS_LEGACY_OMEMO {
            // XEP-0420 forbids a fallback body for SCE; legacy OMEMO 0.3 peers
            // such as Movim need one to accept the stanza at all.
            message = message.append(
                Element::builder("body", ns::JABBER_CLIENT)
                    .append("This message is OMEMO encrypted."),
            );
        }
        let stanza = message
            .append(Element::builder("origin-id", "urn:xmpp:sid:0").attr("id", id.clone()))
            .append(Element::builder("store", "urn:xmpp:hints"))
            .append(
                Element::builder("encryption", "urn:xmpp:eme:0")
                    .attr("namespace", namespace)
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
            namespace: namespace.to_string(),
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
        self.send_reply_in_thread(to, body, reply_to_id, None)
    }

    /// Send a reply and keep it in an XEP-0201 thread when `thread` is set.
    pub fn send_reply_in_thread(
        &self,
        to: &Jid,
        body: &str,
        reply_to_id: &str,
        thread: Option<&str>,
    ) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();
        let mut stanza = Element::builder("message", ns::JABBER_CLIENT)
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
            ));
        if let Some(thread) = thread {
            stanza = stanza.append(Element::builder("thread", ns::JABBER_CLIENT).append(thread));
        }
        self.connection.send_stanza_now(stanza.build())?;

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
        msg.thread = thread.map(str::to_string);

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

    /// Retract another occupant's room message as a moderator (XEP-0425).
    pub fn send_muc_moderation(
        &self,
        room: &Jid,
        message_id: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        self.connection
            .send_stanza_now(crate::muc::moderation_message(room, message_id, reason))?;

        if let Some(mut room) = self.mucs.get(room) {
            room.retract(message_id);
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
        self.fetch_microblog_page(jid, max_items, None)
    }

    /// Fetch a page of microblog entries, using an RSM `after` cursor.
    pub fn fetch_microblog_page(
        &self,
        jid: &Jid,
        max_items: u32,
        after: Option<&str>,
    ) -> Result<()> {
        self.fetch_feed_page(jid, MICROBLOG_NODE, max_items, after)
    }

    /// Fetch a user's active stories (XEP-0501).
    pub fn fetch_stories(&self, jid: &Jid) -> Result<()> {
        self.fetch_feed_page(jid, STORIES_NODE, 50, None)
    }

    /// Fetch a user's stories and merge them into the local cache.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails or the reply is malformed.
    pub async fn fetch_stories_now(&self, jid: &Jid) -> Result<Vec<PubSubItem>> {
        let iq = feed_items_iq(jid, STORIES_NODE, 50, None);
        let reply = self.iq_request(iq).await?;
        let items = reply
            .get_child("pubsub", "http://jabber.org/protocol/pubsub")
            .and_then(|pubsub| pubsub.get_child("items", "http://jabber.org/protocol/pubsub"))
            .ok_or_else(|| XmppError::ProtocolError("no story items returned".into()))?;
        let service = self.pubsub.get_or_create(&Jid::from(jid.to_bare()));
        let mut node = service.get_or_create_node(STORIES_NODE.to_string());
        let parsed: Vec<PubSubItem> = items
            .children()
            .filter(|child| child.name() == "item")
            .filter_map(|item| microblog::parse_item_for_node(item, &jid.to_bare(), STORIES_NODE))
            .collect();
        for item in &parsed {
            if node.get_item(&item.id).is_none() {
                node.add_item(item.clone());
            }
        }
        Ok(parsed)
    }

    /// Refresh own stories and those of mutual roster contacts.
    ///
    /// Individual failures are ignored: one contact without a stories node or
    /// an unreachable service must not stop the rest of the list.
    pub async fn refresh_stories(&self) {
        let Some(own) = self.connection.jid().map(|jid| Jid::from(jid.to_bare())) else {
            return;
        };
        let contacts: Vec<Jid> = self
            .roster
            .list()
            .into_iter()
            .filter(|item| item.subscription == crate::roster::SubscriptionState::Both)
            .map(|item| Jid::from(item.jid.to_bare()))
            .filter(|jid| jid != &own)
            .take(120)
            .collect();
        let _ = self.fetch_stories_now(&own).await;
        for jid in contacts {
            let _ = self.fetch_stories_now(&jid).await;
        }
    }

    fn fetch_feed_page(
        &self,
        jid: &Jid,
        node: &str,
        max_items: u32,
        after: Option<&str>,
    ) -> Result<()> {
        self.connection
            .send_stanza_now(feed_items_iq(jid, node, max_items, after))
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

    /// Retract one of our microblog posts (XEP-0060 retract).
    pub fn retract_microblog(&self, item_id: &str) -> Result<()> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let own_bare = own.to_bare();
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", own_bare.to_string())
            .attr("id", format!("post-del-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("retract", "http://jabber.org/protocol/pubsub")
                        .attr("node", MICROBLOG_NODE)
                        .attr("notify", "true")
                        .append(
                            Element::builder("item", "http://jabber.org/protocol/pubsub")
                                .attr("id", item_id.to_string()),
                        ),
                ),
            )
            .build();
        self.connection.send_stanza_now(iq)?;
        self.pubsub
            .retract(&Jid::from(own_bare), MICROBLOG_NODE, item_id);
        Ok(())
    }

    /// Update one of our microblog posts in place (same item id).
    pub fn update_microblog(
        &self,
        item_id: &str,
        body: &str,
        title: Option<&str>,
        tags: &[String],
        media: &[crate::types::MediaAttachment],
    ) -> Result<PubSubItem> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let own_bare = own.to_bare();
        let service = self.pubsub.get_or_create(&Jid::from(own_bare.clone()));
        let mut node = service.get_or_create_node(MICROBLOG_NODE.to_string());
        let existing = node.get_item(item_id);
        let published = existing
            .as_ref()
            .map(|item| item.published_at)
            .unwrap_or_else(chrono::Utc::now);
        let now = chrono::Utc::now();
        let comments = existing
            .as_ref()
            .and_then(|item| item.comments.clone())
            .or_else(|| self.comments_reference(item_id));
        if let Some(reference) = comments.as_ref() {
            let _ = self.create_comments_node(reference);
        }
        let mut entry = microblog::build_entry_updated(
            item_id,
            body,
            title,
            &own_bare.to_string(),
            published,
            now,
            tags,
            media,
        );
        if let Some(reference) = comments.as_ref() {
            entry = microblog::add_comments_link(entry, &reference.service, &reference.node);
        }
        self.send_microblog_publish(item_id, entry)?;

        let updated = node.update_microblog_item(
            item_id,
            body.to_string(),
            title.map(str::to_string),
            tags.to_vec(),
            media.to_vec(),
            comments.clone(),
            now,
        );
        let mut item = existing.unwrap_or_else(|| {
            PubSubItem::from_microblog(
                MICROBLOG_NODE.to_string(),
                Jid::from(own_bare.clone()),
                body.to_string(),
                tags.to_vec(),
                media.to_vec(),
            )
        });
        item.id = item_id.to_string();
        item.body = Some(body.to_string());
        item.title = title.map(str::to_string);
        item.tags = tags.to_vec();
        item.media = media.to_vec();
        item.comments = comments;
        item.updated_at = Some(now);
        if !updated {
            node.add_item(item.clone());
        }
        Ok(item)
    }

    fn comments_reference(&self, item_id: &str) -> Option<CommentRef> {
        let own = self.connection.jid()?.to_bare();
        Some(CommentRef {
            service: Jid::from(own),
            node: format!("{}{}", microblog::COMMENTS_PREFIX, item_id),
        })
    }

    fn create_comments_node(&self, reference: &CommentRef) -> Result<()> {
        let form = comments_config_form(self.disco.has_service_feature(
            &reference.service.to_string(),
            crate::pubsub::relationships::NS_PUBSUB_RELATIONSHIPS,
        ));
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", reference.service.to_string())
            .attr("id", format!("comments-create-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub")
                    .append(
                        Element::builder("create", "http://jabber.org/protocol/pubsub")
                            .attr("node", reference.node.clone()),
                    )
                    .append(
                        Element::builder("configure", "http://jabber.org/protocol/pubsub")
                            .append(form),
                    ),
            )
            .build();
        self.connection.send_stanza_now(iq)
    }

    fn send_microblog_publish(&self, item_id: &str, entry: Element) -> Result<()> {
        self.send_feed_publish(
            MICROBLOG_NODE,
            item_id,
            entry,
            &[("pubsub#access_model", "presence")],
        )
    }

    fn send_story_publish(&self, item_id: &str, entry: Element) -> Result<()> {
        self.send_feed_publish(
            STORIES_NODE,
            item_id,
            entry,
            &[
                ("pubsub#persist_items", "true"),
                ("pubsub#max_items", "max"),
                ("pubsub#notify_sub", "true"),
                ("pubsub#notify_retract", "true"),
                ("pubsub#access_model", "presence"),
                ("pubsub#item_expire", "86400"),
            ],
        )
    }

    fn send_feed_publish(
        &self,
        node: &str,
        item_id: &str,
        entry: Element,
        options: &[(&str, &str)],
    ) -> Result<()> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let own_bare = own.to_bare();
        let mut form = Element::builder("x", "jabber:x:data")
            .attr("type", "submit")
            .append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", "FORM_TYPE")
                    .attr("type", "hidden")
                    .append(
                        Element::builder("value", "jabber:x:data")
                            .append("http://jabber.org/protocol/pubsub#publish-options"),
                    ),
            );
        for (var, value) in options {
            form = form.append(
                Element::builder("field", "jabber:x:data")
                    .attr("var", *var)
                    .append(Element::builder("value", "jabber:x:data").append(*value)),
            );
        }
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", own_bare.to_string())
            .attr("id", format!("post-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub")
                    .append(
                        Element::builder("publish", "http://jabber.org/protocol/pubsub")
                            .attr("node", node)
                            .append(
                                Element::builder("item", "http://jabber.org/protocol/pubsub")
                                    .attr("id", item_id.to_string())
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
        Ok(())
    }

    /// Fetch the comments node advertised by a post.
    pub fn fetch_comments(&self, reference: &CommentRef) -> Result<()> {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", reference.service.to_string())
            .attr("id", format!("comments-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("items", "http://jabber.org/protocol/pubsub")
                        .attr("node", reference.node.clone())
                        .append(
                            Element::builder("set", "http://jabber.org/protocol/rsm").append(
                                Element::builder("max", "http://jabber.org/protocol/rsm")
                                    .append("50"),
                            ),
                        ),
                ),
            )
            .build();
        self.connection.send_stanza_now(iq)
    }

    /// Publish a comment to the advertised comments node.
    pub fn publish_comment(&self, reference: &CommentRef, body: &str) -> Result<PubSubItem> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let own_bare = own.to_bare();
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let entry =
            microblog::build_entry_with(&id, body, None, &own_bare.to_string(), now, &[], &[]);
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", reference.service.to_string())
            .attr("id", format!("comment-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("publish", "http://jabber.org/protocol/pubsub")
                        .attr("node", reference.node.clone())
                        .append(
                            Element::builder("item", "http://jabber.org/protocol/pubsub")
                                .attr("id", id.clone())
                                .append(entry),
                        ),
                ),
            )
            .build();
        self.connection.send_stanza_now(iq)?;

        let mut item = PubSubItem::from_microblog(
            reference.node.clone(),
            Jid::from(own_bare),
            body.to_string(),
            Vec::new(),
            Vec::new(),
        );
        item.id = id;
        item.published_at = now;
        let service = self.pubsub.get_or_create(&reference.service);
        let mut node = service.get_or_create_node(reference.node.clone());
        node.add_item(item.clone());
        Ok(item)
    }

    /// Publish a post to our own microblog node (XEP-0277).
    pub fn publish_microblog(&self, body: &str, title: Option<&str>) -> Result<PubSubItem> {
        self.publish_microblog_with(body, title, &[], &[])
    }

    /// Publish a post with categories and enclosure attachments.
    pub fn publish_microblog_with(
        &self,
        body: &str,
        title: Option<&str>,
        tags: &[String],
        media: &[crate::types::MediaAttachment],
    ) -> Result<PubSubItem> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let own_bare = own.to_bare();
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let comments = self.comments_reference(&id);
        if let Some(reference) = comments.as_ref() {
            let _ = self.create_comments_node(reference);
        }
        let mut entry =
            microblog::build_entry_with(&id, body, title, &own_bare.to_string(), now, tags, media);
        if let Some(reference) = comments.as_ref() {
            entry = microblog::add_comments_link(entry, &reference.service, &reference.node);
        }
        self.send_microblog_publish(&id, entry)?;

        let mut item = PubSubItem::from_microblog(
            MICROBLOG_NODE.to_string(),
            Jid::from(own_bare.clone()),
            body.to_string(),
            tags.to_vec(),
            media.to_vec(),
        );
        item.id = id;
        item.title = title.map(|t| t.to_string());
        item.published_at = now;
        item.comments = comments;

        let service = self.pubsub.get_or_create(&Jid::from(own_bare));
        let mut node = service.get_or_create_node(MICROBLOG_NODE.to_string());
        node.add_item(item.clone());

        Ok(item)
    }

    /// Publish a story with exactly one image or video attachment (XEP-0501).
    ///
    /// # Errors
    ///
    /// Returns an error when the media is not an image/video, the client is
    /// not connected, or the stanza cannot be queued.
    pub fn publish_story(
        &self,
        caption: &str,
        media: &crate::types::MediaAttachment,
    ) -> Result<PubSubItem> {
        if !matches!(
            media.media_type,
            crate::types::MediaType::Image | crate::types::MediaType::Video
        ) {
            return Err(XmppError::ProtocolError(
                "a story needs one image or video attachment".into(),
            ));
        }
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let own_bare = own.to_bare();
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let tags = story_tags(caption);
        let title = (!caption.trim().is_empty()).then_some(caption);
        let entry = microblog::build_entry_with(
            &id,
            caption,
            title,
            &own_bare.to_string(),
            now,
            &tags,
            std::slice::from_ref(media),
        );
        self.send_story_publish(&id, entry)?;

        let mut item = PubSubItem::from_microblog(
            STORIES_NODE.to_string(),
            Jid::from(own_bare.clone()),
            caption.to_string(),
            tags,
            vec![media.clone()],
        );
        item.id = id;
        item.title = title.map(str::to_string);
        item.published_at = now;
        item.expires_at = Some(now + chrono::Duration::hours(24));

        let service = self.pubsub.get_or_create(&Jid::from(own_bare));
        let mut node = service.get_or_create_node(STORIES_NODE.to_string());
        node.add_item(item.clone());

        Ok(item)
    }

    /// Retract one of our stories.
    ///
    /// # Errors
    ///
    /// Returns an error when the client is not connected or the stanza cannot
    /// be queued.
    pub fn retract_story(&self, item_id: &str) -> Result<()> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let own_bare = own.to_bare();
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", own_bare.to_string())
            .attr("id", format!("story-del-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("retract", "http://jabber.org/protocol/pubsub")
                        .attr("node", STORIES_NODE)
                        .attr("notify", "true")
                        .append(
                            Element::builder("item", "http://jabber.org/protocol/pubsub")
                                .attr("id", item_id.to_string()),
                        ),
                ),
            )
            .build();
        self.connection.send_stanza_now(iq)?;
        self.pubsub
            .retract(&Jid::from(own_bare), STORIES_NODE, item_id);
        Ok(())
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

    /// Publish the legacy OMEMO 0.3 device list.
    ///
    /// # Errors
    ///
    /// Returns an error when not connected or the publish fails.
    pub fn publish_legacy_omemo_device_list(&self, device_ids: &[u32]) -> Result<()> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let xml = omemo_stanza::axolotl_stanza::DeviceList {
            devices: device_ids.to_vec(),
        }
        .encode()
        .map_err(|e| XmppError::ProtocolError(e.to_string()))?;
        let element = Element::from_reader(xml.as_bytes())
            .map_err(|e| XmppError::DeserializationError(e.to_string()))?;
        self.publish_pep(
            &own.to_bare(),
            omemo::NS_LEGACY_DEVICES,
            Some("current"),
            element,
        )
    }

    /// Publish this device's legacy OMEMO 0.3 bundle.
    ///
    /// # Errors
    ///
    /// Returns an error when not connected or the publish fails.
    pub fn publish_legacy_omemo_bundle(
        &self,
        bundle: &crate::omemo::LegacyDeviceBundle,
    ) -> Result<()> {
        let own = self.connection.jid().ok_or(XmppError::NotConnected)?;
        let node = format!("{}{}", omemo::NS_LEGACY_BUNDLES_PREFIX, bundle.device_id);
        self.publish_pep(&own.to_bare(), &node, Some("current"), bundle.to_element()?)
    }

    /// Fetch a contact's legacy OMEMO 0.3 device list.
    ///
    /// # Errors
    ///
    /// Returns an error when the IQ cannot be sent.
    pub fn fetch_legacy_omemo_device_list(&self, jid: &Jid) -> Result<()> {
        self.fetch_pep_items(jid, omemo::NS_LEGACY_DEVICES, 1)
    }

    /// Fetch a contact's legacy OMEMO 0.3 device list and bundles.
    ///
    /// # Errors
    ///
    /// Returns an error when the device-list IQ cannot be sent.
    pub fn fetch_legacy_omemo(&self, jid: &Jid) -> Result<()> {
        self.fetch_legacy_omemo_device_list(jid)
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
        let _ = self.broadcast_presence(None);
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
                use base64::{engine::general_purpose::STANDARD, Engine as _};
                use sha1::{Digest, Sha1};
                if let Ok(bytes) = STANDARD.decode(data.trim()) {
                    let digest = Sha1::digest(&bytes);
                    item.avatar_hash = Some(
                        digest
                            .iter()
                            .map(|byte| format!("{byte:02x}"))
                            .collect::<String>(),
                    );
                }
                item.avatar_data = Some(data);
            }
            if let Some(mime) = crate::vcard::field_text(photo, "TYPE") {
                item.avatar_mime = Some(mime);
            }
        }
        item.vcard = Some(xml);
        self.roster.upsert(item);
        let _ = self.broadcast_presence(None);
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
        self.set_presence_with_idle(show, status, priority, None)
    }

    /// Broadcast a presence update with an optional XEP-0319 idle timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error when the stanza cannot be queued.
    pub fn set_presence_with_idle(
        &self,
        show: Option<&str>,
        status: Option<&str>,
        priority: Option<i32>,
        idle_since: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<()> {
        let mut config = self.connection.config();
        config.presence_show = show.map(str::to_string);
        config.presence_status = status.map(str::to_string);
        if let Some(priority) = priority {
            config.priority = priority;
        }
        self.connection.set_config(config.clone());

        self.broadcast_presence(idle_since)
    }

    /// Send the current configured presence, including our vCard avatar hash.
    fn broadcast_presence(&self, idle_since: Option<chrono::DateTime<chrono::Utc>>) -> Result<()> {
        let config = self.connection.config();
        let own = self.connection.jid().map(|jid| jid.to_bare());
        let item = own.as_ref().and_then(|jid| self.roster.get(jid));
        let nick = item.as_ref().and_then(|item| item.vcard_field("NICKNAME"));
        let avatar_hash = item.as_ref().and_then(|item| item.avatar_hash.clone());
        self.connection
            .send_stanza_now(crate::presence::presence_element_full(
                config.presence_show.as_deref(),
                config.presence_status.as_deref(),
                config.priority,
                nick.as_deref(),
                idle_since,
                avatar_hash.as_deref(),
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

    /// Join a room unless a join is already in flight or the room is joined.
    ///
    /// Rooms restored from storage arrive as `Disconnected` with their old nick,
    /// so opening one must send presence again. Returns whether presence was
    /// sent (`false` when the room was already joined or joining).
    ///
    /// # Errors
    ///
    /// Returns an error when the presence stanza cannot be sent.
    pub fn ensure_room_joined(&self, room: &Jid, nick: &str) -> Result<bool> {
        if let Some(existing) = self.mucs.get(room) {
            if !existing.needs_join() {
                return Ok(false);
            }
        }
        self.join_room(room, nick, None)?;
        Ok(true)
    }

    /// [`ensure_room_joined`](Self::ensure_room_joined) for a JID given as text.
    ///
    /// # Errors
    ///
    /// Returns an error when the room JID is invalid or presence cannot be sent.
    pub fn ensure_room_joined_str(&self, room: &str, nick: &str) -> Result<bool> {
        let jid: Jid = room
            .trim()
            .parse()
            .map_err(|e: jid::Error| XmppError::InvalidJid(e.to_string()))?;
        self.ensure_room_joined(&jid, nick)
    }

    /// Fetch the owner, admin and member lists for a room (XEP-0045).
    ///
    /// Used by room OMEMO to include offline members in the recipient set.
    pub fn request_muc_affiliations(&self, room: &Jid) -> Result<()> {
        for affiliation in ["owner", "admin", "member"] {
            let iq = Element::builder("iq", ns::JABBER_CLIENT)
                .attr("type", "get")
                .attr("to", room.to_bare().to_string())
                .attr(
                    "id",
                    format!("muc-affiliations-{}-{}", affiliation, Uuid::new_v4()),
                )
                .append(
                    Element::builder("query", "http://jabber.org/protocol/muc#admin").append(
                        Element::builder("item", "http://jabber.org/protocol/muc#admin")
                            .attr("affiliation", affiliation),
                    ),
                )
                .build();
            self.connection.send_stanza_now(iq)?;
        }
        Ok(())
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

    /// Publish a XEP-0402 bookmark, optionally pinned with XEP-0469.
    ///
    /// The bookmark is written to our own PEP node and mirrored locally so the
    /// room list updates without waiting for the server notification.
    pub fn publish_bookmark(
        &self,
        room: &Jid,
        name: &str,
        nick: &str,
        autojoin: bool,
        pinned: bool,
    ) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        let item = crate::muc::bookmark_item(room, name, nick, autojoin, pinned);
        let form = Element::builder("x", crate::data_form::NS_DATA_FORMS)
            .attr("type", "submit")
            .append(
                Element::builder("field", crate::data_form::NS_DATA_FORMS)
                    .attr("var", "FORM_TYPE")
                    .attr("type", "hidden")
                    .append(
                        Element::builder("value", crate::data_form::NS_DATA_FORMS)
                            .append("http://jabber.org/protocol/pubsub#publish-options"),
                    ),
            )
            .append(
                Element::builder("field", crate::data_form::NS_DATA_FORMS)
                    .attr("var", "pubsub#persist_items")
                    .append(
                        Element::builder("value", crate::data_form::NS_DATA_FORMS).append("true"),
                    ),
            )
            .append(
                Element::builder("field", crate::data_form::NS_DATA_FORMS)
                    .attr("var", "pubsub#max_items")
                    .append(
                        Element::builder("value", crate::data_form::NS_DATA_FORMS).append("max"),
                    ),
            )
            .append(
                Element::builder("field", crate::data_form::NS_DATA_FORMS)
                    .attr("var", "pubsub#access_model")
                    .append(
                        Element::builder("value", crate::data_form::NS_DATA_FORMS)
                            .append("whitelist"),
                    ),
            );
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", own.to_string())
            .attr("id", format!("bookmark-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub")
                    .append(
                        Element::builder("publish", "http://jabber.org/protocol/pubsub")
                            .attr("node", crate::spaces::NS_BOOKMARKS)
                            .append(item),
                    )
                    .append(
                        Element::builder("publish-options", "http://jabber.org/protocol/pubsub")
                            .append(form),
                    ),
            )
            .build();
        self.connection.send_stanza_now(iq)?;
        self.mucs.set_bookmark(
            &room.to_bare().to_string(),
            name.to_string(),
            nick.to_string(),
            autojoin,
            pinned,
        );
        Ok(())
    }

    /// Pin or unpin a room bookmark (XEP-0469).
    ///
    /// Pinning a room that is not bookmarked publishes a bookmark as well.
    pub fn set_room_pinned(&self, room: &Jid, pinned: bool) -> Result<()> {
        let existing = self.mucs.get(room);
        let name = existing
            .as_ref()
            .map(|room| room.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| room.to_bare().to_string());
        let nick = existing
            .as_ref()
            .and_then(|room| room.self_nick())
            .or_else(|| {
                self.connection
                    .jid()
                    .and_then(|jid| jid.node().map(|node| node.to_string()))
            })
            .unwrap_or_else(|| "me".to_string());
        let autojoin = existing.as_ref().is_some_and(|room| room.is_autojoin());
        self.publish_bookmark(room, &name, &nick, autojoin, pinned)
    }

    /// Remove a room from the bookmark list.
    pub fn remove_bookmark(&self, room: &Jid) -> Result<()> {
        let own = self
            .connection
            .jid()
            .ok_or(XmppError::NotConnected)?
            .to_bare();
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("to", own.to_string())
            .attr("id", format!("bookmark-del-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("retract", "http://jabber.org/protocol/pubsub")
                        .attr("node", crate::spaces::NS_BOOKMARKS)
                        .attr("notify", "true")
                        .append(
                            Element::builder("item", "http://jabber.org/protocol/pubsub")
                                .attr("id", room.to_bare().to_string()),
                        ),
                ),
            )
            .build();
        self.connection.send_stanza_now(iq)?;
        self.mucs.clear_bookmark(&room.to_bare().to_string());
        Ok(())
    }

    /// Search a XEP-0433 channel search service.
    ///
    /// `after` is the `last` cursor from a previous page; an empty result is a
    /// valid reply and is not treated as an error.
    pub async fn search_channels(
        &self,
        service: &Jid,
        query: &str,
        max: usize,
        after: Option<&str>,
    ) -> Result<ChannelSearchPage> {
        let iq = crate::channel_search::search_query(service, query, max, after);
        let reply = self.iq_request(iq).await?;
        let result = reply
            .get_child("result", crate::channel_search::NS_CHANNEL_SEARCH)
            .ok_or_else(|| XmppError::ProtocolError("channel search reply has no result".into()))?;
        Ok(crate::channel_search::parse_results(result))
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

    /// Send an XEP-0410 self-ping to our own occupant in a joined room.
    ///
    /// # Errors
    ///
    /// Returns an error when the room is not joined, when its nick is unknown,
    /// or when the IQ times out or is rejected.
    pub async fn ping_muc_room(&self, room: &Jid) -> Result<()> {
        let target = self
            .mucs
            .get(room)
            .and_then(|room| room.self_ping_target())
            .ok_or_else(|| XmppError::ProtocolError("room is not joined".into()))?;
        self.iq_request(crate::muc::self_ping_iq(&target)).await?;
        Ok(())
    }

    /// Self-ping every joined room, returning the result per room.
    ///
    /// A failed ping marks the room stale (disconnected) so the UI stops
    /// treating it as live; the next connection will rejoin it.
    pub async fn ping_muc_rooms(&self) -> Vec<(Jid, Result<()>)> {
        let rooms: Vec<Jid> = self
            .mucs
            .list_joined()
            .into_iter()
            .map(|room| room.jid)
            .collect();
        let mut results = Vec::with_capacity(rooms.len());
        for room in rooms {
            let result = self.ping_muc_room(&room).await;
            if result.is_err() {
                if let Some(joined) = self.mucs.get(&room) {
                    joined.mark_disconnected();
                }
            }
            results.push((room, result));
        }
        results
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

/// Build a `disco#items` request for the spaces service.
///
/// When the service advertises XEP-0462, the query asks for space nodes
/// directly instead of probing every node with a separate `disco#info`.
fn spaces_items_iq(service: &Jid, filtered: bool) -> Result<Element> {
    let filter = filtered.then(|| {
        crate::pubsub::discovery::NodeTypeFilter::included(vec![
            crate::spaces::NS_SPACES.to_string()
        ])
    });
    let query = crate::pubsub::discovery::items_query(filter.as_ref())?;
    Ok(Element::builder("iq", ns::JABBER_CLIENT)
        .attr("type", "get")
        .attr("to", service.to_string())
        .attr("id", format!("spaces-items-{}", Uuid::new_v4()))
        .append(query)
        .build())
}

/// Build the node configuration for a microblog comments node.
///
/// The XEP-0496 `parent` field is only included when the service advertises
/// support; unknown configuration fields can make a server reject the whole
/// configuration request.
fn comments_config_form(with_relationship: bool) -> Element {
    let form = Element::builder("x", crate::data_form::NS_DATA_FORMS)
        .attr("type", "submit")
        .append(
            Element::builder("field", crate::data_form::NS_DATA_FORMS)
                .attr("var", "FORM_TYPE")
                .attr("type", "hidden")
                .append(
                    Element::builder("value", crate::data_form::NS_DATA_FORMS)
                        .append("http://jabber.org/protocol/pubsub#node_config"),
                ),
        )
        .append(
            Element::builder("field", crate::data_form::NS_DATA_FORMS)
                .attr("var", "pubsub#access_model")
                .append(Element::builder("value", crate::data_form::NS_DATA_FORMS).append("open")),
        )
        .append(
            Element::builder("field", crate::data_form::NS_DATA_FORMS)
                .attr("var", "pubsub#max_items")
                .append(Element::builder("value", crate::data_form::NS_DATA_FORMS).append("max")),
        )
        .append(
            Element::builder("field", crate::data_form::NS_DATA_FORMS)
                .attr("var", "pubsub#persist_items")
                .append(Element::builder("value", crate::data_form::NS_DATA_FORMS).append("true")),
        )
        .append(
            Element::builder("field", crate::data_form::NS_DATA_FORMS)
                .attr("var", "pubsub#notify_retract")
                .append(Element::builder("value", crate::data_form::NS_DATA_FORMS).append("true")),
        );
    if with_relationship {
        form.append(crate::pubsub::relationships::parent_config_field(
            crate::microblog::MICROBLOG_NODE,
        ))
        .build()
    } else {
        form.build()
    }
}

/// Build a PubSub `items` request for a feed node, with optional RSM paging.
fn feed_items_iq(jid: &Jid, node: &str, max_items: u32, after: Option<&str>) -> Element {
    let mut rsm = Element::builder("set", "http://jabber.org/protocol/rsm").append(
        Element::builder("max", "http://jabber.org/protocol/rsm").append(max_items.to_string()),
    );
    if let Some(after) = after.filter(|after| !after.is_empty()) {
        rsm = rsm.append(
            Element::builder("after", "http://jabber.org/protocol/rsm").append(after.to_string()),
        );
    }
    Element::builder("iq", ns::JABBER_CLIENT)
        .attr("type", "get")
        .attr("to", jid.to_bare().to_string())
        .attr("id", format!("feed-{}", Uuid::new_v4()))
        .append(
            Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                Element::builder("items", "http://jabber.org/protocol/pubsub")
                    .attr("node", node)
                    .append(rsm),
            ),
        )
        .build()
}

/// Extract `#hashtags` from a story caption.
fn story_tags(caption: &str) -> Vec<String> {
    caption
        .split_whitespace()
        .filter_map(|word| word.strip_prefix('#'))
        .map(|tag| {
            tag.trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                .to_string()
        })
        .filter(|tag| !tag.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use minidom::NSChoice;

    #[test]
    fn spaces_items_query_can_filter_by_node_type() {
        let service: Jid = "pubsub.example.org".parse().expect("service");
        let plain = spaces_items_iq(&service, false).expect("iq");
        let query = plain
            .get_child("query", "http://jabber.org/protocol/disco#items")
            .expect("query");
        assert!(query
            .get_child("filter", crate::pubsub::discovery::NS_PUBSUB_FILTER)
            .is_none());

        let filtered = spaces_items_iq(&service, true).expect("iq");
        let query = filtered
            .get_child("query", "http://jabber.org/protocol/disco#items")
            .expect("query");
        let filter = query
            .get_child("filter", crate::pubsub::discovery::NS_PUBSUB_FILTER)
            .expect("filter");
        let parsed = crate::pubsub::discovery::parse_filter(filter).expect("parsed");
        assert_eq!(
            parsed,
            crate::pubsub::discovery::NodeTypeFilter::included(vec![
                crate::spaces::NS_SPACES.to_string()
            ])
        );
    }

    #[test]
    fn extracts_story_hashtags() {
        assert_eq!(
            story_tags("Sunset #sky #sea, #2026!"),
            vec!["sky".to_string(), "sea".to_string(), "2026".to_string()]
        );
    }

    #[test]
    fn comments_config_only_declares_a_parent_when_supported() {
        let with = comments_config_form(true);
        let parent = with
            .children()
            .find(|child| child.attr("var") == Some(crate::pubsub::relationships::FIELD_PARENT))
            .expect("parent field");
        assert_eq!(
            parent
                .get_child("value", crate::data_form::NS_DATA_FORMS)
                .map(minidom::Element::text)
                .as_deref(),
            Some(crate::microblog::MICROBLOG_NODE)
        );

        let without = comments_config_form(false);
        assert!(without
            .children()
            .all(|child| child.attr("var") != Some(crate::pubsub::relationships::FIELD_PARENT)));
    }

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
    fn space_image_item_carries_avatar_metadata() {
        let item = space_image_item(
            crate::spaces::NS_SPACE_AVATAR,
            "https://files.example/avatar.jpg",
            "image/jpeg",
            b"abc",
            Some(64),
            Some(48),
        );
        assert_eq!(item.name(), "item");
        assert_eq!(item.attr("id"), Some(crate::spaces::NS_SPACE_AVATAR));
        let metadata = item
            .get_child("metadata", "urn:xmpp:avatar:metadata")
            .expect("metadata");
        let info = metadata
            .get_child("info", "urn:xmpp:avatar:metadata")
            .expect("info");
        assert_eq!(info.attr("url"), Some("https://files.example/avatar.jpg"));
        assert_eq!(info.attr("type"), Some("image/jpeg"));
        assert_eq!(info.attr("bytes"), Some("3"));
        assert_eq!(info.attr("width"), Some("64"));
        assert_eq!(info.attr("height"), Some("48"));
        assert_eq!(
            info.attr("id"),
            Some("a9993e364706816aba3e25717850c26c9cd0d89d")
        );
    }

    #[test]
    fn space_room_item_roundtrips() {
        let room = crate::spaces::SpaceRoom {
            conference: "dev@conference.example.org".to_string(),
            name: "development".to_string(),
            kind: crate::spaces::SpaceRoomKind::Voice,
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
        assert_eq!(parsed.kind, crate::spaces::SpaceRoomKind::Voice);
    }
}
