//! XMPP Event Processor - handles incoming XMPP events and updates local state

use jid::Jid;
use minidom::{Element, NSChoice};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};
use uuid::Uuid;
use xmpp_parsers::ns;

use crate::{
    chat::{
        ChatMessage, ChatStateNotification, ForwardInfo, MessageHints, MessageReceipt, MessageType,
    },
    connection::ConnectionState,
    disco::{DiscoManager, ExternalService, ServerInfo, UploadService, UploadSlot},
    iq::{parse_iq_error, IqError, IqRouter},
    mam::MamManager,
    muc::{MucAffiliation, MucMessage, MucOccupant, MucRole, PresenceShow as MucPresenceShow},
    omemo::{DeviceBundle, LegacyOmemoEnvelope, OmemoEnvelope, OmemoManager},
    push::PushManager,
    roster::{CapsInfo, Presence, PresenceShow, RosterItem, SubscriptionState},
    sm::StreamManagement,
    types::{MediaAttachment, MediaType},
    ChatManager, Connection, ConnectionEvent, MucManager, PubSubManager, Result, Roster,
    SpaceManager, XmppError,
};

const NS_RECEIPTS: &str = "urn:xmpp:receipts";
const NS_CHATSTATES: &str = "http://jabber.org/protocol/chatstates";
const NS_CHAT_MARKERS: &str = "urn:xmpp:chat-markers:0";
const NS_MUC_USER: &str = "http://jabber.org/protocol/muc#user";
const NS_ROSTER: &str = "jabber:iq:roster";
const NS_BOOKMARKS: &str = "urn:xmpp:bookmarks:1";
const NS_BOOKMARKS_LEGACY: &str = "storage:bookmarks";
const NS_EXTERNAL_SERVICES: &str = "urn:xmpp:extdisco:2";
const NS_CSI: &str = "urn:xmpp:csi:0";
const NS_HINTS: &str = "urn:xmpp:hints";
const NS_MAM: &str = "urn:xmpp:mam:2";
const NS_RSM: &str = "http://jabber.org/protocol/rsm";
const NS_BLOCKING: &str = "urn:xmpp:blocking";
const NS_OOB: &str = "jabber:x:oob";
const NS_FILE_METADATA: &str = "urn:xmpp:file:metadata:0";
const NS_FORWARD: &str = "urn:xmpp:forward:0";
const NS_ATTENTION: &str = "urn:xmpp:attention:0";
const NS_FALLBACK: &str = "urn:xmpp:fallback:0";

/// Host callback for UI notifications. Implemented by the presentation layer.
pub trait ProcessorNotifier {
    fn bump(&self);
    fn set_connection_state(&self, state: ConnectionState);
}

/// Event processor that handles incoming XMPP events and updates local state
pub struct EventProcessor {
    connection: Arc<Connection>,
    roster: Arc<Roster>,
    chats: Arc<ChatManager>,
    mucs: Arc<MucManager>,
    pubsub: Arc<PubSubManager>,
    spaces: Arc<SpaceManager>,
    disco: Arc<DiscoManager>,
    push: Arc<PushManager>,
    sm: Arc<StreamManagement>,
    omemo: Arc<OmemoManager>,
    iq: Arc<IqRouter>,
    mam: Arc<MamManager>,
    jingle: Arc<crate::jingle::JingleManager>,
    conference: Arc<crate::conference::ConferenceManager>,
    own_jid: Arc<tokio::sync::RwLock<Option<Jid>>>,
    notifier: Option<Arc<dyn ProcessorNotifier>>,
}

impl EventProcessor {
    pub fn new(
        connection: Arc<Connection>,
        roster: Arc<Roster>,
        chats: Arc<ChatManager>,
        mucs: Arc<MucManager>,
        pubsub: Arc<PubSubManager>,
        spaces: Arc<SpaceManager>,
        disco: Arc<DiscoManager>,
        push: Arc<PushManager>,
        sm: Arc<StreamManagement>,
        omemo: Arc<OmemoManager>,
        iq: Arc<IqRouter>,
        mam: Arc<MamManager>,
    ) -> Self {
        let jingle = crate::jingle::JingleManager::new(
            connection.clone(),
            iq.clone(),
            Arc::new(crate::EventBus::new()),
        );
        Self::new_with_jingle(
            connection, roster, chats, mucs, pubsub, spaces, disco, push, sm, omemo, iq, mam,
            jingle,
        )
    }

    /// Construct a processor with the conference manager shared with the UI.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_jingle_and_conference(
        connection: Arc<Connection>,
        roster: Arc<Roster>,
        chats: Arc<ChatManager>,
        mucs: Arc<MucManager>,
        pubsub: Arc<PubSubManager>,
        spaces: Arc<SpaceManager>,
        disco: Arc<DiscoManager>,
        push: Arc<PushManager>,
        sm: Arc<StreamManagement>,
        omemo: Arc<OmemoManager>,
        iq: Arc<IqRouter>,
        mam: Arc<MamManager>,
        jingle: Arc<crate::jingle::JingleManager>,
        conference: Arc<crate::conference::ConferenceManager>,
    ) -> Self {
        Self {
            connection,
            roster,
            chats,
            mucs,
            pubsub,
            spaces,
            disco,
            push,
            sm,
            omemo,
            iq,
            mam,
            jingle,
            conference,
            own_jid: Arc::new(tokio::sync::RwLock::new(None)),
            notifier: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_jingle(
        connection: Arc<Connection>,
        roster: Arc<Roster>,
        chats: Arc<ChatManager>,
        mucs: Arc<MucManager>,
        pubsub: Arc<PubSubManager>,
        spaces: Arc<SpaceManager>,
        disco: Arc<DiscoManager>,
        push: Arc<PushManager>,
        sm: Arc<StreamManagement>,
        omemo: Arc<OmemoManager>,
        iq: Arc<IqRouter>,
        mam: Arc<MamManager>,
        jingle: Arc<crate::jingle::JingleManager>,
    ) -> Self {
        Self::new_with_jingle_and_conference(
            connection,
            roster,
            chats,
            mucs,
            pubsub,
            spaces,
            disco,
            push,
            sm,
            omemo,
            iq,
            mam,
            jingle,
            crate::conference::ConferenceManager::new(),
        )
    }

    /// Attach a presentation-layer notifier.
    pub fn with_notifier(mut self, notifier: Arc<dyn ProcessorNotifier>) -> Self {
        self.notifier = Some(notifier);
        self
    }

    fn bump(&self) {
        if let Some(notifier) = &self.notifier {
            notifier.bump();
        }
    }

    fn set_connection_state(&self, state: ConnectionState) {
        if let Some(notifier) = &self.notifier {
            notifier.set_connection_state(state);
        }
    }

    pub async fn start(
        self,
        mut event_rx: tokio::sync::mpsc::UnboundedReceiver<ConnectionEvent>,
    ) -> Result<()> {
        info!("Event processor started");
        // XEP-0410: a room can silently lose our session while the stream stays
        // up. Ping our own occupant periodically so a dead room is noticed.
        let mut keepalive = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(60),
            Duration::from_secs(60),
        );
        keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                event = event_rx.recv() => {
                    let Some(event) = event else {
                        break;
                    };
                    if let Err(e) = self.handle_connection_event(event).await {
                        warn!("Failed to handle connection event: {}", e);
                    }
                    // Data may have changed (state, roster, chats, MUCs) - refresh the UI.
                    self.bump();
                }
                _ = keepalive.tick() => {
                    self.ping_joined_rooms();
                }
            }
        }
        Ok(())
    }

    fn ping_joined_rooms(&self) {
        let mucs = self.mucs.clone();
        let iq = self.iq.clone();
        let connection = self.connection.clone();
        tokio::spawn(async move {
            for room in mucs.list_joined() {
                let Some(target) = room.self_ping_target() else {
                    continue;
                };
                let result = iq
                    .request(
                        |stanza| connection.send_stanza_now(stanza.clone()),
                        crate::muc::self_ping_iq(&target),
                    )
                    .await;
                if result.is_err() {
                    room.mark_disconnected();
                    debug!(room = %room.jid, "MUC self-ping failed; room marked stale");
                }
            }
        });
    }

    async fn handle_connection_event(&self, event: ConnectionEvent) -> Result<()> {
        match event {
            ConnectionEvent::Connected { jid } => {
                info!("Connected as {}", jid);
                if let Ok(parsed_jid) = jid.parse::<Jid>() {
                    self.jingle.set_own_jid(parsed_jid.clone());
                    *self.own_jid.write().await = Some(parsed_jid);
                }
                self.set_connection_state(ConnectionState::Connected);
                // XEP-0198 must be enabled before other traffic so counters line up,
                // unless this stream was resumed and keeps the old counters.
                if self.sm.take_resumed() {
                    info!("Resumed the previous stream; keeping stream-management counters");
                } else {
                    self.sm.reset();
                    if self.connection.config().enable_stream_management {
                        self.request_stream_management();
                    }
                }
                // Announce presence and fetch the roster so the client comes alive.
                self.enable_carbons();
                self.send_initial_presence();
                // A room session lives in the stream, so a new stream has to
                // join every room that was live before the reconnect; otherwise
                // its messages silently stop arriving.
                for (room, nick) in self.mucs.take_rejoin_rooms() {
                    let presence = Element::builder("presence", ns::JABBER_CLIENT)
                        .attr("to", format!("{}/{}", room.to_bare(), nick))
                        .append(Element::builder("x", "http://jabber.org/protocol/muc"))
                        .build();
                    if self.connection.send_stanza_now(presence).is_ok() {
                        if let Some(mut stored) = self.mucs.get(&room) {
                            let _ = stored.join(&nick, None);
                        }
                    } else {
                        warn!(%room, "could not rejoin the room after reconnecting");
                    }
                }
                self.publish_omemo_device();
                self.request_roster();
                self.request_own_vcard();
                self.request_bookmarks();
                self.discover_server();
                if self.disco.has_feature(NS_EXTERNAL_SERVICES) {
                    self.request_external_services();
                }
                if self.disco.has_feature(NS_CSI) {
                    let _ = self.connection.send_csi_state();
                }
                if self.disco.has_feature(NS_BLOCKING) {
                    self.roster.request_blocklist();
                }
            }
            ConnectionEvent::Authenticated => {
                info!("Authenticated");
                self.set_connection_state(ConnectionState::Authenticated);
            }
            ConnectionEvent::Disconnected { reason } => {
                info!("Disconnected: {}", reason);
                self.sm.connection_lost();
                self.iq.fail_all(&IqError::Disconnected);
                self.jingle.clear();
                *self.own_jid.write().await = None;
                // Room sessions die with the stream: remember the rooms that
                // were live so the next connection can join them again.
                self.mucs.mark_all_disconnected();
                self.set_connection_state(ConnectionState::Disconnected);
            }
            ConnectionEvent::Reconnecting { attempt } => {
                info!("Reconnecting (attempt {})", attempt);
                self.set_connection_state(ConnectionState::Reconnecting { attempt });
            }
            ConnectionEvent::ReconnectTriggered => {
                info!("Reconnect triggered");
                let connection = self.connection.clone();
                tokio::spawn(async move {
                    if let Err(e) = connection.connect().await {
                        warn!("Reconnect failed: {}", e);
                    }
                });
            }
            ConnectionEvent::ReconnectFailed => {
                warn!("Reconnection failed");
                self.set_connection_state(ConnectionState::Failed {
                    reason: "Reconnection failed".to_string(),
                });
            }
            ConnectionEvent::StanzaReceived(xml) => {
                debug!("Received stanza: {}", xml);
                self.set_connection_state(ConnectionState::Authenticated);
                if let Err(e) = self.handle_stanza(&xml).await {
                    warn!("Failed to handle stanza: {}", e);
                }
            }
            ConnectionEvent::Error(err) => {
                warn!("Connection error: {}", err);
                self.set_connection_state(ConnectionState::Failed { reason: err });
            }
        }
        Ok(())
    }

    /// Send initial presence so the server starts delivering roster/presence updates.
    fn send_initial_presence(&self) {
        let config = self.connection.config();
        let nick = self
            .connection
            .jid()
            .and_then(|jid| self.roster.get(&jid.to_bare()))
            .and_then(|item| item.vcard_field("NICKNAME"));
        let presence = crate::presence::presence_element(
            config.presence_show.as_deref(),
            config.presence_status.as_deref(),
            config.priority,
            nick.as_deref(),
        );
        if let Err(e) = self.connection.send_stanza_now(presence) {
            warn!("Failed to send initial presence: {}", e);
        }
    }

    /// Enable message carbons (XEP-0280) so other resources' messages are copied here.
    fn enable_carbons(&self) {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "set")
            .attr("id", format!("carbons-{}", Uuid::new_v4()))
            .append(Element::builder("enable", "urn:xmpp:carbons:2"))
            .build();
        if let Err(e) = self.connection.send_stanza_now(iq) {
            warn!("Failed to enable carbons: {}", e);
        }
    }

    /// Ask the server for our roster.
    fn request_roster(&self) {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("id", "roster-init")
            .append(Element::builder("query", NS_ROSTER))
            .build();
        if let Err(e) = self.connection.send_stanza_now(iq) {
            warn!("Failed to request roster: {}", e);
        }
    }

    fn request_external_services(&self) {
        let Some(own) = self.connection.jid() else {
            return;
        };
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", own.domain().to_string())
            .attr("id", "extdisco-init")
            .append(Element::builder("services", NS_EXTERNAL_SERVICES))
            .build();
        if let Err(e) = self.connection.send_stanza_now(iq) {
            warn!("Failed to request external services: {}", e);
        }
    }

    fn discover_server(&self) {
        let Some(own) = self.connection.jid() else {
            return;
        };
        let items = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", own.domain().to_string())
            .attr("id", "disco-init")
            .append(Element::builder(
                "query",
                "http://jabber.org/protocol/disco#items",
            ))
            .build();
        if let Err(e) = self.connection.send_stanza_now(items) {
            warn!("Failed to start service discovery: {}", e);
        }

        let info = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", own.domain().to_string())
            .attr("id", "disco-server-info")
            .append(Element::builder(
                "query",
                "http://jabber.org/protocol/disco#info",
            ))
            .build();
        if let Err(e) = self.connection.send_stanza_now(info) {
            warn!("Failed to query server features: {}", e);
        }
    }

    fn publish_omemo_device(&self) {
        self.omemo.rotate_signed_prekey_if_due(14 * 24 * 60 * 60);
        self.omemo.replenish_prekeys(50, 100);
        let Some(local) = self.omemo.local() else {
            return;
        };

        let mut ids = self
            .connection
            .jid()
            .map(|j| self.omemo.device_list(&j.to_bare().to_string()))
            .unwrap_or_default();
        if !ids.contains(&local.device_id) {
            ids.push(local.device_id);
        }
        let legacy_ids = ids.clone();
        let mut devices = Element::builder("devices", crate::omemo::NS_OMEMO);
        for id in ids {
            devices = devices.append(
                Element::builder("device", crate::omemo::NS_OMEMO).attr("id", id.to_string()),
            );
        }
        let devices = devices.build();
        self.publish_pep(crate::omemo::NS_DEVICES, Some("current"), devices);
        self.publish_pep(
            crate::omemo::NS_BUNDLES,
            Some(&local.device_id.to_string()),
            local.bundle().to_element(),
        );

        if let Ok(xml) = (omemo_stanza::axolotl_stanza::DeviceList {
            devices: legacy_ids,
        })
        .encode()
        {
            if let Ok(element) = Element::from_reader(xml.as_bytes()) {
                self.publish_pep(crate::omemo::NS_LEGACY_DEVICES, Some("current"), element);
            }
        }
        if let Ok(element) = local.legacy_bundle().to_element() {
            let node = format!(
                "{}{}",
                crate::omemo::NS_LEGACY_BUNDLES_PREFIX,
                local.device_id
            );
            self.publish_pep(&node, Some("current"), element);
        }
    }

    fn publish_pep(&self, node: &str, item_id: Option<&str>, payload: Element) {
        let own = match self.connection.jid() {
            Some(jid) => Jid::from(jid.to_bare()),
            None => return,
        };
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
            .attr("to", own.to_string())
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
        if let Err(e) = self.connection.send_stanza_now(iq) {
            warn!("Failed to publish PEP node {}: {}", node, e);
        }
    }

    fn request_stream_management(&self) {
        let enable = Element::builder("enable", "urn:xmpp:sm:3")
            .attr("resume", "true")
            .build();
        if let Err(e) = self.connection.send_stanza_now(enable) {
            warn!("Failed to enable stream management: {}", e);
        }
    }

    fn request_bookmarks(&self) {
        let Some(own) = self.connection.jid() else {
            return;
        };
        let own = Jid::from(own.to_bare());
        let pep = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", own.to_string())
            .attr("id", format!("bookmarks-{}", Uuid::new_v4()))
            .append(
                Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                    Element::builder("items", "http://jabber.org/protocol/pubsub")
                        .attr("node", NS_BOOKMARKS),
                ),
            )
            .build();
        let _ = self.connection.send_stanza_now(pep);

        let legacy = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("id", format!("bookmarks-legacy-{}", Uuid::new_v4()))
            .append(
                Element::builder("query", "jabber:iq:private")
                    .append(Element::builder("storage", NS_BOOKMARKS_LEGACY)),
            )
            .build();
        let _ = self.connection.send_stanza_now(legacy);
    }

    fn request_spaces(&self, service: &str) {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", service.to_string())
            .attr("id", format!("spaces-items-{}", Uuid::new_v4()))
            .append(Element::builder(
                "query",
                "http://jabber.org/protocol/disco#items",
            ))
            .build();
        if let Err(e) = self.connection.send_stanza_now(iq) {
            debug!("No spaces service available: {}", e);
        }
    }

    fn request_space_info(&self, id: &crate::spaces::SpaceId) {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", id.server.clone())
            .attr("id", format!("space-info-{}", Uuid::new_v4()))
            .append(
                Element::builder("query", "http://jabber.org/protocol/disco#info")
                    .attr("node", id.node.clone()),
            )
            .build();
        let _ = self.connection.send_stanza_now(iq);
    }

    fn request_space_items(&self, id: &crate::spaces::SpaceId) {
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
        let _ = self.connection.send_stanza_now(iq);
    }

    fn ingest_space_items(&self, items: &Element, server: &str, node: &str) {
        let id = crate::spaces::SpaceId::new(server, node);
        let mut rooms = Vec::new();
        let mut directories = None;
        let mut avatar = None;
        let mut banner = None;
        // A notification carries only what changed, so merge it into the list
        // instead of replacing it: adding a second room must not drop the first.
        let retracted: Vec<String> = items
            .children()
            .filter(|child| child.name() == "retract")
            .filter_map(|child| child.attr("id").map(str::to_string))
            .collect();
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
                directories = Some(crate::parse_space_directories(node));
                continue;
            }
            if let Some(conference) = item.get_child("conference", crate::spaces::NS_BOOKMARKS) {
                if let Some(room) = crate::parse_space_room(item_id, conference) {
                    rooms.push(room);
                }
            }
        }
        self.spaces
            .set_metadata(&id, None, None, avatar, None, None);
        self.spaces.set_banner(&id, banner);
        if let Some(directories) = directories {
            self.spaces.set_directories(&id, directories);
        }
        if !rooms.is_empty() || !retracted.is_empty() {
            self.spaces.merge_rooms(&id, rooms, &retracted);
        }
        self.spaces.mark_known(id);
    }

    fn request_own_vcard(&self) {
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("id", "vcard-own-init")
            .append(Element::builder("vCard", "vcard-temp"))
            .build();
        if let Err(e) = self.connection.send_stanza_now(iq) {
            warn!("Failed to request own vCard: {}", e);
        }
    }

    async fn handle_stanza(&self, xml: &str) -> Result<()> {
        let elem =
            Element::from_str(xml).map_err(|e| XmppError::ParseError(format!("{}: {}", e, xml)))?;

        // XEP-0198 nonzas are not counted as handled stanzas.
        if elem.ns() == "urn:xmpp:sm:3" {
            match elem.name() {
                "enabled" => {
                    let id = elem.attr("id").map(|s| s.to_string());
                    let resume = elem.attr("resume") == Some("true");
                    let max = elem.attr("max").and_then(|m| m.parse::<u32>().ok());
                    self.sm.enable(id, resume, max);
                    return Ok(());
                }
                "r" => {
                    let ack = Element::builder("a", "urn:xmpp:sm:3")
                        .attr("h", self.sm.handled().to_string())
                        .build();
                    if let Err(e) = self.connection.send_stanza_now(ack) {
                        warn!("Failed to send stream-management ack: {}", e);
                    }
                    return Ok(());
                }
                "a" => {
                    if let Some(h) = elem.attr("h").and_then(|v| v.parse::<u64>().ok()) {
                        self.sm.set_acked(h);
                    }
                    return Ok(());
                }
                "failed" => {
                    warn!("Stream management failed; continuing without it");
                    self.sm.reset();
                    return Ok(());
                }
                _ => {}
            }
        }

        let result = match elem.name() {
            "message" => self.handle_message(&elem, false).await,
            "presence" => self.handle_presence(&elem).await,
            "iq" => self.handle_iq(&elem).await,
            _ => Ok(()),
        };

        if self.sm.is_enabled() && matches!(elem.name(), "message" | "presence" | "iq") {
            self.sm.note_handled();
        }

        result
    }

    async fn handle_message(&self, elem: &Element, sent_carbon: bool) -> Result<()> {
        // XEP-0280: message carbons wrap the real message in <sent/> or <received/>.
        if let Some(carbon) = elem
            .get_child("sent", "urn:xmpp:carbons:2")
            .or_else(|| elem.get_child("received", "urn:xmpp:carbons:2"))
        {
            if let Some(inner) = carbon
                .get_child("forwarded", "urn:xmpp:forward:0")
                .and_then(|f| f.get_child("message", NSChoice::Any))
            {
                let sent = elem.get_child("sent", "urn:xmpp:carbons:2").is_some();
                return Box::pin(self.handle_message(inner, sent)).await;
            }
            return Ok(());
        }

        // XEP-0353: Jingle Message Initiation (call ringing or conference
        // proceed/reject).
        if let Some(child) = elem
            .children()
            .find(|child| child.ns() == crate::jingle::NS_JINGLE_MESSAGE)
        {
            let conference = child
                .attr("id")
                .is_some_and(|id| self.conference.owns_sid(id));
            if conference {
                let update = self.conference.handle_message(elem)?;
                for event in update.events {
                    if let crate::conference::ConferenceEvent::Proceed { sid } = event {
                        self.start_conference_media(&sid).await;
                    }
                }
            } else {
                self.jingle.handle_jmi_message(elem).await?;
            }
            return Ok(());
        }

        // XEP-0060: PubSub event notification (e.g. a new microblog post).
        if let Some(event) = elem.get_child("event", "http://jabber.org/protocol/pubsub#event") {
            let service = elem
                .attr("from")
                .and_then(|f| f.parse::<Jid>().ok())
                .map(|j| Jid::from(j.to_bare()));
            if let Some(items) = event.get_child("items", NSChoice::Any) {
                let node_id = items.attr("node").unwrap_or_default();
                if let Some(service_jid) = service.as_ref() {
                    let server = service_jid.to_bare().to_string();
                    if self.spaces.is_known_node(&server, node_id) {
                        self.ingest_space_items(items, &server, node_id);
                    }
                }
                self.ingest_pubsub_items(items, service);
            }
            return Ok(());
        }

        // XEP-0313: archived message result.
        if let Some(result) = elem.get_child("result", "urn:xmpp:mam:2") {
            self.handle_mam_result(result).await?;
            return Ok(());
        }

        let from = elem.attr("from").unwrap_or_default().to_string();
        let msg_type = elem.attr("type").unwrap_or("normal").to_string();
        let from_jid: Option<Jid> = from.parse().ok();
        let is_muc = msg_type == "groupchat" || elem.get_child("x", NS_MUC_USER).is_some();
        let to_jid: Option<Jid> = elem.attr("to").and_then(|s| s.parse().ok());
        let own_bare = self.own_jid.read().await.as_ref().map(|j| j.to_bare());
        let from_is_own = own_bare.is_some() && from_jid.as_ref().map(|j| j.to_bare()) == own_bare;
        let reply_to: Option<String> = elem
            .get_child("reply", "urn:xmpp:reply:0")
            .and_then(|r| r.attr("id"))
            .map(|s| s.to_string());
        let thread: Option<String> = elem
            .get_child("thread", NSChoice::Any)
            .map(|thread| thread.text())
            .filter(|thread| !thread.is_empty());
        let explicit_encryption = parse_encryption(elem);

        let muc_parts = if is_muc { from.split_once('/') } else { None };
        let muc_room_jid = muc_parts
            .and_then(|(room, _)| room.parse::<Jid>().ok())
            .map(|room| room.to_bare());
        let muc_nick = muc_parts.map(|(_, nick)| nick).unwrap_or("");

        let omemo_peer = if is_muc {
            muc_room_jid
                .as_ref()
                .and_then(|room| self.mucs.get(room))
                .and_then(|room| room.get_occupant(muc_nick))
                .and_then(|occupant| occupant.jid)
        } else if from_is_own {
            to_jid.clone().or_else(|| from_jid.clone())
        } else {
            from_jid.clone()
        };

        // --- XEP-0444: Reactions ---
        if let Some(reactions) = elem.get_child("reactions", "urn:xmpp:reactions:0") {
            if let Some(target_id) = reactions.attr("id") {
                let emojis: Vec<String> = reactions
                    .children()
                    .filter(|c| c.name() == "reaction")
                    .map(|c| c.text())
                    .filter(|s| !s.is_empty())
                    .collect();
                if let Some(room) = muc_room_jid.as_ref() {
                    let mut room = self.mucs.get_or_create(room);
                    room.set_reactions_from(target_id, muc_nick, &emojis);
                } else if let Some(peer) = from_jid.as_ref() {
                    let mut chat = self.chats.get_or_create(peer);
                    chat.set_reactions_from(target_id, peer, &emojis);
                }
            }
        }

        // --- XEP-0308: Last message correction ---
        if let Some(replace_id) = elem
            .get_child("replace", "urn:xmpp:message-correct:0")
            .and_then(|r| r.attr("id"))
        {
            if let Some(new_body) = elem.get_child("body", NSChoice::Any).map(|b| b.text()) {
                if let Some(room) = muc_room_jid.as_ref() {
                    let mut room = self.mucs.get_or_create(room);
                    room.apply_edit(replace_id, &new_body);
                } else if let Some(peer) = from_jid.as_ref() {
                    let mut chat = self.chats.get_or_create(peer);
                    chat.apply_edit(replace_id, &new_body);
                }
            }
        }

        // --- XEP-0425: Message moderation ---
        // A moderator's broadcast carries the original message id in
        // `<apply-to>`; retract it and do not add the moderation notice as a
        // new message.
        if let Some(apply_to) = elem.get_child("apply-to", "urn:xmpp:fasten:0") {
            if apply_to
                .get_child("moderated", "urn:xmpp:message-moderate:1")
                .is_some()
            {
                if let (Some(target_id), Some(room)) = (apply_to.attr("id"), muc_room_jid.as_ref())
                {
                    let mut room = self.mucs.get_or_create(room);
                    room.retract(target_id);
                }
                return Ok(());
            }
        }

        // --- XEP-0424: Message retraction ---
        if let Some(retract_id) = elem
            .get_child("retract", "urn:xmpp:message-retract:0")
            .and_then(|r| r.attr("id"))
        {
            if let Some(room) = muc_room_jid.as_ref() {
                let mut room = self.mucs.get_or_create(room);
                room.retract(retract_id);
            } else if let Some(peer) = from_jid.as_ref() {
                let mut chat = self.chats.get_or_create(peer);
                chat.retract(retract_id);
            }
        }

        // --- XEP-0428: Fallback indication ---
        // Reactions, edits, retractions and moderation are often sent with a
        // human-readable body for legacy clients. Once the extension has been
        // applied, that body must not become a new chat message.
        let fallback_for = elem
            .get_child("fallback", NS_FALLBACK)
            .and_then(|fallback| fallback.attr("for"));
        let fallback_only = elem
            .get_child("reactions", "urn:xmpp:reactions:0")
            .is_some()
            || elem
                .get_child("retract", "urn:xmpp:message-retract:0")
                .is_some()
            || elem
                .get_child("replace", "urn:xmpp:message-correct:0")
                .is_some()
            || matches!(
                fallback_for,
                Some(
                    "urn:xmpp:reactions:0"
                        | "urn:xmpp:message-retract:0"
                        | "urn:xmpp:message-correct:0"
                        | "urn:xmpp:message-moderate:1"
                )
            );
        if fallback_only {
            return Ok(());
        }
        let mut body = elem
            .get_child("body", NSChoice::Any)
            .map(|b| b.text())
            .filter(|b| !b.is_empty());

        // XEP-0384: decrypt OMEMO payloads for display.
        let mut omemo_encrypted = false;
        let mut omemo_decrypted = false;
        if let Some(encrypted) = elem.get_child("encrypted", crate::omemo::NS_OMEMO) {
            omemo_encrypted = true;
            {
                if let (Some(peer), Some(envelope)) =
                    (omemo_peer.as_ref(), OmemoEnvelope::from_element(encrypted))
                {
                    let result = if is_muc {
                        match (own_bare.as_ref(), muc_room_jid.as_ref()) {
                            (Some(own), Some(room)) => self.omemo.decrypt_group_message(
                                &peer.to_bare().to_string(),
                                &own.to_string(),
                                &room.to_string(),
                                &envelope,
                            ),
                            _ => Err(crate::XmppError::CryptoError(
                                "no local JID or room for group OMEMO".into(),
                            )),
                        }
                    } else {
                        self.omemo
                            .decrypt_message(&peer.to_bare().to_string(), &envelope)
                    };
                    match result {
                        Ok(text) => {
                            omemo_decrypted = true;
                            // Empty OMEMO messages only advance the ratchet.
                            body = (!text.is_empty()).then_some(text);
                            if let Some(bundle) = self.omemo.replenish_prekeys(20, 80) {
                                self.publish_pep(
                                    crate::omemo::NS_BUNDLES,
                                    Some(&bundle.device_id.to_string()),
                                    bundle.to_element(),
                                );
                            }
                        }
                        Err(e) => debug!("Failed to decrypt OMEMO message: {}", e),
                    }
                }
            }
            if body.is_none() && !omemo_decrypted {
                body = Some("[Unable to decrypt OMEMO message]".to_string());
            }
        } else if let Some(encrypted) = elem.get_child("encrypted", crate::omemo::NS_LEGACY_OMEMO) {
            omemo_encrypted = true;
            if let (Some(peer), Some(envelope)) = (
                omemo_peer.as_ref(),
                LegacyOmemoEnvelope::from_element(encrypted),
            ) {
                match self
                    .omemo
                    .decrypt_legacy_message(&peer.to_bare().to_string(), &envelope)
                {
                    Ok(text) => {
                        omemo_decrypted = true;
                        body = (!text.is_empty()).then_some(text);
                    }
                    Err(e) => debug!("Failed to decrypt legacy OMEMO message: {}", e),
                }
            }
            if body.is_none() && !omemo_decrypted {
                body = Some("[Unable to decrypt OMEMO message]".to_string());
            }
        }

        // --- XEP-0184: Message Delivery Receipts ---
        if elem.get_child("request", NS_RECEIPTS).is_some() {
            if let (Some(to), Some(request_id)) = (from_jid.as_ref(), elem.attr("id")) {
                let receipt = Element::builder("message", ns::JABBER_CLIENT)
                    .attr("to", to.to_string())
                    .attr("id", format!("rec-{}", Uuid::new_v4()))
                    .append(Element::builder("received", NS_RECEIPTS).attr("id", request_id))
                    .append(Element::builder("no-store", NS_HINTS))
                    .append(Element::builder("no-copy", NS_HINTS))
                    .build();
                let _ = self.connection.send_stanza_now(receipt);
            }
        }

        if let Some(received_id) = elem
            .get_child("received", NS_RECEIPTS)
            .and_then(|c| c.attr("id"))
        {
            if let Some(peer) = from_jid.as_ref() {
                let mut chat = self.chats.get_or_create(peer);
                chat.set_receipt(received_id, MessageReceipt::Delivered);
            }
        }

        // --- XEP-0333: Chat markers (displayed) ---
        if let Some(displayed_id) = elem
            .get_child("displayed", NS_CHAT_MARKERS)
            .and_then(|c| c.attr("id"))
        {
            if let Some(peer) = from_jid.as_ref() {
                let mut chat = self.chats.get_or_create(peer);
                chat.set_receipt(displayed_id, MessageReceipt::Displayed);
            }
        }

        // --- XEP-0085: Chat state notifications ---
        if let Some(state) = chat_state_from(elem) {
            if let Some(peer) = from_jid.as_ref() {
                let mut chat = self.chats.get_or_create(peer);
                chat.set_peer_state(state);
            }
        }

        // --- XEP-0045: room subject (a groupchat message without a body) ---
        if msg_type == "groupchat" {
            if let Some(subject_el) = elem.get_child("subject", NSChoice::Any) {
                if let Ok(room_jid) = from.split('/').next().unwrap_or("").parse::<Jid>() {
                    self.mucs.set_subject(
                        &room_jid.to_bare().to_string(),
                        subject_el.text(),
                        from_jid.clone(),
                    );
                }
            }
        }

        // --- XEP-0249 / XEP-0045: invitations surface the room in the list ---
        if let Some(invite) = elem.get_child("x", "jabber:x:conference") {
            if let Some(room) = invite.attr("jid").and_then(|j| j.parse::<Jid>().ok()) {
                self.mucs.get_or_create(&room);
            }
            return Ok(());
        }
        if let Some(muc_user) = elem.get_child("x", NS_MUC_USER) {
            if muc_user.get_child("invite", NSChoice::Any).is_some() {
                if let Ok(room) = from.parse::<Jid>() {
                    self.mucs.get_or_create(&room);
                }
                return Ok(());
            }
        }

        let forwarded = elem.get_child("forwarded", NS_FORWARD);
        let forwarded_inner = forwarded.and_then(|f| f.get_child("message", NSChoice::Any));
        let forward_info = parse_forward_info(elem);
        let mut media = parse_media(elem);
        let sticker = crate::bob::parse_sticker(elem);
        let attention = elem.get_child("attention", NS_ATTENTION).is_some();
        let mut mentions = parse_mentions(elem);
        let mut story_ref = parse_story_ref(elem);
        if media.is_empty() {
            if let Some(inner) = forwarded_inner {
                media = parse_media(inner);
            }
        }
        if mentions.is_empty() {
            if let Some(inner) = forwarded_inner {
                mentions = parse_mentions(inner);
            }
        }
        if story_ref.is_none() {
            if let Some(inner) = forwarded_inner {
                story_ref = parse_story_ref(inner);
            }
        }
        let Some(body) = body
            .or_else(|| {
                forwarded_inner
                    .and_then(|inner| inner.get_child("body", NSChoice::Any))
                    .map(|body| body.text())
                    .filter(|body| !body.is_empty())
            })
            .or_else(|| {
                media
                    .first()
                    .and_then(|attachment| attachment.name.clone())
                    .or_else(|| media.first().map(|attachment| attachment.url.clone()))
            })
            .or_else(|| sticker.as_ref().map(|_| "[Sticker]".to_string()))
            .or_else(|| attention.then(|| "Attention".to_string()))
        else {
            return Ok(());
        };

        let (origin_id, stanza_id) = message_ids(elem);
        let hints = message_hints(elem);
        let ids: Vec<String> = origin_id.iter().chain(stanza_id.iter()).cloned().collect();
        let message_id = origin_id.clone().or_else(|| stanza_id.clone());

        if is_muc {
            let room_part = from.split('/').next().unwrap_or(&from);
            let nick = from.split('/').nth(1).unwrap_or("");
            let Ok(room_jid) = room_part.parse::<Jid>() else {
                return Ok(());
            };

            if let Some(update) = self.conference.observe_presence(elem) {
                match update {
                    crate::conference::VoicePresence::Discovered(discovery) => {
                        tracing::info!(
                            room = %discovery.room,
                            sfu = %discovery.jid,
                            dialect = ?discovery.dialect,
                            "conference SFU discovered"
                        );
                    }
                    crate::conference::VoicePresence::Ended { room } => {
                        tracing::info!(%room, "conference SFU left the room");
                    }
                }
            }

            let mut room = self.mucs.get_or_create(&room_jid);
            if room.contains_any_id(ids.iter().map(String::as_str)) {
                return Ok(());
            }
            let is_own = room.self_nick().as_deref() == Some(nick);
            if is_own && room.is_echo(nick, &body) {
                return Ok(());
            }
            let mut msg = MucMessage::new(
                room.bare_jid.clone(),
                nick.to_string(),
                from_jid.clone(),
                is_own,
                Some(body),
            );
            if let Some(id) = message_id {
                msg.id = id;
            }
            msg.origin_id = origin_id;
            msg.stanza_id = stanza_id;
            msg.hints = hints;
            msg.media = media.clone();
            msg.mentions = mentions.clone();
            msg.attention = attention;
            if let Some((cid, alt)) = sticker.clone() {
                msg.sticker_cid = Some(cid);
                msg.sticker_alt = Some(alt);
            }
            msg.forward_info = forward_info.clone();
            msg.reply_to = reply_to.clone();
            msg.thread = thread.clone();
            msg.encryption = if omemo_encrypted {
                Some(crate::types::EncryptionInfo {
                    namespace: crate::omemo::NS_OMEMO.to_string(),
                    name: "OMEMO".to_string(),
                    fingerprint: None,
                })
            } else {
                explicit_encryption.clone()
            };
            room.add_message(msg);
        } else {
            let is_own = sent_carbon || from_is_own;
            let peer = if is_own {
                to_jid.clone().or_else(|| from_jid.clone())
            } else {
                from_jid.clone()
            };
            let Some(peer) = peer else {
                return Ok(());
            };

            let mut chat = self.chats.get_or_create(&peer);
            if chat.contains_any_id(ids.iter().map(String::as_str)) {
                return Ok(());
            }
            let mut msg = ChatMessage::new(
                peer.to_bare().to_string(),
                from_jid.clone().unwrap_or_else(|| peer.clone()),
                to_jid.unwrap_or_else(|| peer.clone()),
                is_own,
                Some(body),
            );
            if let Some(id) = message_id {
                msg.id = id;
            }
            msg.origin_id = origin_id;
            msg.stanza_id = stanza_id;
            msg.hints = hints;
            msg.media = media;
            msg.mentions = mentions;
            msg.attention = attention;
            msg.story_ref = story_ref;
            if let Some((cid, alt)) = sticker {
                msg.sticker_cid = Some(cid);
                msg.sticker_alt = Some(alt);
            }
            msg.forward_info = forward_info;
            if omemo_encrypted {
                msg.encryption = Some(crate::types::EncryptionInfo {
                    namespace: crate::omemo::NS_OMEMO.to_string(),
                    name: "OMEMO".to_string(),
                    fingerprint: None,
                });
            } else {
                msg.encryption = explicit_encryption;
            }
            msg.message_type = match msg_type.as_str() {
                "chat" => MessageType::Chat,
                "groupchat" => MessageType::GroupChat,
                "headline" => MessageType::Headline,
                "error" => MessageType::Error,
                _ => MessageType::Normal,
            };
            msg.reply_to = reply_to;
            msg.thread = thread;
            chat.add_message(msg);
        }

        Ok(())
    }

    /// Add an archived (MAM) message to its conversation, deduplicating by id.
    async fn handle_mam_result(&self, result: &Element) -> Result<()> {
        let Some(forwarded) = result.get_child("forwarded", "urn:xmpp:forward:0") else {
            return Ok(());
        };
        let Some(inner) = forwarded.get_child("message", NSChoice::Any) else {
            return Ok(());
        };

        let from_jid: Option<Jid> = inner.attr("from").and_then(|s| s.parse().ok());
        let to_jid: Option<Jid> = inner.attr("to").and_then(|s| s.parse().ok());
        let own_bare = self.own_jid.read().await.as_ref().map(|j| j.to_bare());

        let (peer, is_own) = match (&from_jid, &to_jid, &own_bare) {
            (Some(f), Some(t), Some(o)) if &f.to_bare() == o => (t.clone(), true),
            (Some(f), _, _) => (f.clone(), false),
            (_, Some(t), _) => (t.clone(), false),
            _ => return Ok(()),
        };

        let archive_id = result.attr("id").unwrap_or("").to_string();
        let (origin_id, stanza_id) = message_ids(inner);
        let reply_to: Option<String> = inner
            .get_child("reply", "urn:xmpp:reply:0")
            .and_then(|reply| reply.attr("id"))
            .map(str::to_string);
        let thread: Option<String> = inner
            .get_child("thread", NSChoice::Any)
            .map(|thread| thread.text())
            .filter(|thread| !thread.is_empty());
        let omemo_encrypted = inner
            .get_child("encrypted", crate::omemo::NS_OMEMO)
            .is_some();
        let explicit_encryption = parse_encryption(inner);
        let encryption = if omemo_encrypted {
            Some(crate::types::EncryptionInfo {
                namespace: crate::omemo::NS_OMEMO.to_string(),
                name: "OMEMO".to_string(),
                fingerprint: None,
            })
        } else {
            explicit_encryption
        };
        let hints = message_hints(inner);
        let ids: Vec<String> = origin_id
            .iter()
            .chain(stanza_id.iter())
            .cloned()
            .chain((!archive_id.is_empty()).then(|| archive_id.clone()))
            .collect();
        let id = origin_id
            .clone()
            .or_else(|| stanza_id.clone())
            .unwrap_or_else(|| {
                if archive_id.is_empty() {
                    Uuid::new_v4().to_string()
                } else {
                    archive_id.clone()
                }
            });

        let timestamp = forwarded
            .get_child("delay", "urn:xmpp:delay")
            .and_then(|d| d.attr("stamp"))
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        // Room archives: the forwarded message is a groupchat message.
        if inner.attr("type") == Some("groupchat") {
            let media = parse_media(inner);
            let mentions = parse_mentions(inner);
            let forward_info = parse_forward_info(inner);
            let sticker = crate::bob::parse_sticker(inner);
            let mut body = inner
                .get_child("body", NSChoice::Any)
                .map(|b| b.text())
                .filter(|b| !b.is_empty())
                .or_else(|| {
                    media
                        .first()
                        .and_then(|attachment| attachment.name.clone())
                        .or_else(|| media.first().map(|attachment| attachment.url.clone()))
                });
            let from = inner.attr("from").unwrap_or_default();
            let room_part = from.split('/').next().unwrap_or(&from);
            let nick = from.split('/').nth(1).unwrap_or("");
            let Ok(room_jid) = room_part.parse::<Jid>() else {
                return Ok(());
            };
            let mut room = self.mucs.get_or_create(&room_jid);
            if room.contains_any_id(ids.iter().map(String::as_str)) {
                return Ok(());
            }
            let is_own = room.self_nick().as_deref() == Some(nick);
            if body.is_none() {
                if let Some(encrypted) = inner.get_child("encrypted", crate::omemo::NS_OMEMO) {
                    if let Some(envelope) = OmemoEnvelope::from_element(encrypted) {
                        let sender = room
                            .get_occupant(nick)
                            .and_then(|occupant| occupant.jid)
                            .map(|jid| jid.to_bare());
                        if let (Some(sender), Some(own)) = (sender, own_bare.as_ref()) {
                            body = self
                                .omemo
                                .decrypt_group_message(
                                    &sender.to_string(),
                                    &own.to_string(),
                                    &room_jid.to_string(),
                                    &envelope,
                                )
                                .ok()
                                .filter(|text| !text.is_empty());
                        }
                    }
                } else if let Some(encrypted) =
                    inner.get_child("encrypted", crate::omemo::NS_LEGACY_OMEMO)
                {
                    if let Some(envelope) = LegacyOmemoEnvelope::from_element(encrypted) {
                        let sender = room
                            .get_occupant(nick)
                            .and_then(|occupant| occupant.jid)
                            .map(|jid| jid.to_bare());
                        if let Some(sender) = sender {
                            body = self
                                .omemo
                                .decrypt_legacy_message(&sender.to_string(), &envelope)
                                .ok()
                                .filter(|text| !text.is_empty());
                        }
                    }
                }
            }
            let Some(body) = body else {
                return Ok(());
            };
            let mut msg = MucMessage::new(
                room.bare_jid.clone(),
                nick.to_string(),
                from.parse::<Jid>().ok(),
                is_own,
                Some(body),
            );
            msg.id = id;
            msg.origin_id = origin_id;
            msg.stanza_id = stanza_id;
            msg.hints = hints;
            msg.media = media;
            msg.mentions = mentions;
            msg.forward_info = forward_info;
            msg.reply_to = reply_to.clone();
            msg.thread = thread.clone();
            msg.encryption = encryption.clone();
            if let Some((cid, alt)) = sticker {
                msg.sticker_cid = Some(cid);
                msg.sticker_alt = Some(alt);
            }
            msg.timestamp = timestamp;
            room.add_message(msg);
            if !archive_id.is_empty() {
                room.note_archive_id(timestamp, &archive_id);
            }
            return Ok(());
        }

        let mut chat = self.chats.get_or_create(&peer);
        if chat.contains_any_id(ids.iter().map(String::as_str)) {
            return Ok(());
        }

        let mut body = inner
            .get_child("body", NSChoice::Any)
            .map(|b| b.text())
            .filter(|b| !b.is_empty());
        if body.is_none() {
            if let Some(encrypted) = inner.get_child("encrypted", crate::omemo::NS_OMEMO) {
                let decrypted = OmemoEnvelope::from_element(encrypted).and_then(|envelope| {
                    self.omemo
                        .decrypt_message(&peer.to_bare().to_string(), &envelope)
                        .ok()
                });
                body = Some(
                    decrypted.unwrap_or_else(|| "[Unable to decrypt OMEMO message]".to_string()),
                );
            } else if let Some(encrypted) =
                inner.get_child("encrypted", crate::omemo::NS_LEGACY_OMEMO)
            {
                let decrypted = LegacyOmemoEnvelope::from_element(encrypted).and_then(|envelope| {
                    self.omemo
                        .decrypt_legacy_message(&peer.to_bare().to_string(), &envelope)
                        .ok()
                });
                body = Some(
                    decrypted.unwrap_or_else(|| "[Unable to decrypt OMEMO message]".to_string()),
                );
            }
        }
        let media = parse_media(inner);
        let mentions = parse_mentions(inner);
        let story_ref = parse_story_ref(inner);
        let forward_info = parse_forward_info(inner);
        let sticker = crate::bob::parse_sticker(inner);
        let attention = inner.get_child("attention", NS_ATTENTION).is_some();
        let Some(body) = body.or_else(|| {
            media
                .first()
                .and_then(|attachment| attachment.name.clone())
                .or_else(|| media.first().map(|attachment| attachment.url.clone()))
        }) else {
            return Ok(());
        };

        let mut msg = ChatMessage::new(
            peer.to_bare().to_string(),
            peer.clone(),
            peer.clone(),
            is_own,
            Some(body),
        );
        msg.id = id;
        msg.origin_id = origin_id;
        msg.stanza_id = stanza_id;
        msg.hints = hints;
        msg.media = media;
        msg.mentions = mentions;
        msg.attention = attention;
        msg.story_ref = story_ref;
        msg.forward_info = forward_info;
        msg.reply_to = reply_to;
        msg.thread = thread;
        msg.encryption = encryption;
        if let Some((cid, alt)) = sticker {
            msg.sticker_cid = Some(cid);
            msg.sticker_alt = Some(alt);
        }
        msg.timestamp = timestamp;
        chat.add_message(msg);
        if !archive_id.is_empty() {
            chat.note_archive_id(timestamp, &archive_id);
        }
        Ok(())
    }

    async fn handle_presence(&self, elem: &Element) -> Result<()> {
        let from = elem.attr("from").unwrap_or_default().to_string();
        let ptype = elem.attr("type").unwrap_or("available").to_string();

        let show = show_from(elem);
        let status = elem
            .get_child("status", NSChoice::Any)
            .map(|s| s.text())
            .filter(|s| !s.is_empty());

        // MUC presence?
        if let Some(x) = elem.get_child("x", NS_MUC_USER) {
            let room_part = from.split('/').next().unwrap_or(&from);
            let nick = from.split('/').nth(1).unwrap_or("");
            let Ok(room_jid) = room_part.parse::<Jid>() else {
                return Ok(());
            };

            if let Some(update) = self.conference.observe_presence(elem) {
                match update {
                    crate::conference::VoicePresence::Discovered(discovery) => {
                        tracing::info!(
                            room = %discovery.room,
                            sfu = %discovery.jid,
                            dialect = ?discovery.dialect,
                            "conference SFU discovered"
                        );
                    }
                    crate::conference::VoicePresence::Ended { room } => {
                        tracing::info!(%room, "conference SFU left the room");
                    }
                }
            }

            let mut room = self.mucs.get_or_create(&room_jid);

            match ptype.as_str() {
                "unavailable" => {
                    room.remove_occupant(nick);
                }
                _ => {
                    let mut occupant = MucOccupant::new(nick.to_string());
                    occupant.presence.show = muc_show(show);
                    occupant.presence.status = status;

                    if let Some(item) = x.get_child("item", NSChoice::Any) {
                        if let Some(role) = item.attr("role") {
                            occupant.role = match role {
                                "moderator" => MucRole::Moderator,
                                "visitor" => MucRole::Visitor,
                                "none" => MucRole::None,
                                _ => MucRole::Participant,
                            };
                        }
                        if let Some(aff) = item.attr("affiliation") {
                            occupant.affiliation = match aff {
                                "owner" => MucAffiliation::Owner,
                                "admin" => MucAffiliation::Admin,
                                "member" => MucAffiliation::Member,
                                "outcast" => MucAffiliation::Outcast,
                                _ => MucAffiliation::None,
                            };
                        }
                        if let Some(real_jid) = item.attr("jid") {
                            occupant.jid = real_jid.parse().ok();
                        }
                    }

                    if room.self_nick().as_deref() == Some(nick) {
                        room.mark_joined();
                        room.set_self_role(occupant.role);
                        room.set_self_affiliation(occupant.affiliation);
                        if room.affiliation_jids().is_empty() {
                            for affiliation in ["owner", "admin", "member"] {
                                let iq = Element::builder("iq", ns::JABBER_CLIENT)
                                    .attr("type", "get")
                                    .attr("to", room_jid.to_bare().to_string())
                                    .attr(
                                        "id",
                                        format!(
                                            "muc-affiliations-{}-{}",
                                            affiliation,
                                            Uuid::new_v4()
                                        ),
                                    )
                                    .append(
                                        Element::builder(
                                            "query",
                                            "http://jabber.org/protocol/muc#admin",
                                        )
                                        .append(
                                            Element::builder(
                                                "item",
                                                "http://jabber.org/protocol/muc#admin",
                                            )
                                            .attr("affiliation", affiliation),
                                        ),
                                    )
                                    .build();
                                let _ = self.connection.send_stanza_now(iq);
                            }
                        }
                    }
                    room.add_occupant(occupant);
                }
            }

            return Ok(());
        }

        // Regular presence: update the roster item, preserving roster fields.
        let Ok(from_jid) = from.parse::<Jid>() else {
            return Ok(());
        };

        let mut item = self
            .roster
            .get(&from_jid)
            .unwrap_or_else(|| RosterItem::new(from_jid.clone()));

        match ptype.as_str() {
            "subscribe" => {
                item.is_pending_in = true;
                self.roster.upsert(item);
                self.roster.notify_subscription_request(&from_jid);
            }
            "subscribed" => {
                item.subscription = match item.subscription {
                    SubscriptionState::From | SubscriptionState::Both => SubscriptionState::Both,
                    _ => SubscriptionState::To,
                };
                item.is_pending_out = false;
                item.ask = None;
                self.roster.upsert(item);
                self.roster.notify_subscription_approved(&from_jid);
            }
            "unsubscribe" => {
                item.subscription = match item.subscription {
                    SubscriptionState::Both | SubscriptionState::To => SubscriptionState::To,
                    _ => SubscriptionState::None,
                };
                item.is_pending_in = false;
                self.roster.upsert(item);
            }
            "unsubscribed" => {
                item.subscription = match item.subscription {
                    SubscriptionState::Both | SubscriptionState::From => SubscriptionState::From,
                    _ => SubscriptionState::None,
                };
                item.is_pending_out = false;
                item.ask = None;
                self.roster.upsert(item);
                self.roster.notify_subscription_denied(&from_jid);
            }
            "unavailable" => {
                let mut presence = Presence::new(from_jid);
                presence.show = PresenceShow::Unavailable;
                presence.status = status;
                presence.resource = from.split('/').nth(1).map(|s| s.to_string());
                item.update_presence(presence);
                self.roster.upsert(item);
            }
            _ => {
                let mut presence = Presence::new(from_jid);
                presence.show = show;
                presence.status = status;
                presence.priority = elem
                    .get_child("priority", NSChoice::Any)
                    .and_then(|p| p.text().parse().ok())
                    .unwrap_or(0);
                presence.idle_since = elem
                    .get_child("idle", crate::presence::NS_IDLE)
                    .and_then(|idle| idle.attr("since"))
                    .and_then(|since| chrono::DateTime::parse_from_rfc3339(since).ok())
                    .map(|since| since.with_timezone(&chrono::Utc));
                let vcard_avatar = elem
                    .get_child("x", crate::presence::NS_VCARD_UPDATE)
                    .and_then(|update| update.get_child("photo", crate::presence::NS_VCARD_UPDATE))
                    .map(|photo| photo.text().trim().to_string())
                    .filter(|hash| !hash.is_empty());
                if let Some(hash) = vcard_avatar {
                    if item.avatar_hash.as_deref() != Some(hash.as_str())
                        || item.avatar_data.is_none()
                    {
                        let iq = Element::builder("iq", ns::JABBER_CLIENT)
                            .attr("type", "get")
                            .attr("to", presence.from.to_bare().to_string())
                            .attr("id", format!("vcard-{}", Uuid::new_v4()))
                            .append(Element::builder("vCard", "vcard-temp"))
                            .build();
                        let _ = self.connection.send_stanza_now(iq);
                    }
                }
                if let Some(caps) = elem.get_child("c", "http://jabber.org/protocol/caps") {
                    if let (Some(node), Some(ver)) = (caps.attr("node"), caps.attr("ver")) {
                        item.caps = Some(CapsInfo {
                            node: node.to_string(),
                            ver: ver.to_string(),
                            hash: caps.attr("hash").map(str::to_string),
                        });
                    }
                }
                presence.resource = from.split('/').nth(1).map(|s| s.to_string());
                item.update_presence(presence);
                self.roster.upsert(item);
            }
        }

        Ok(())
    }

    /// Store microblog items from a PubSub `<items>` element (query or event).
    fn ingest_pubsub_items(&self, items: &Element, service: Option<Jid>) {
        let node_id = items.attr("node").unwrap_or_default();
        let service_bare = service
            .or_else(|| self.connection.jid().map(|j| Jid::from(j.to_bare())))
            .map(|j| j.to_bare().to_string())
            .unwrap_or_default();

        if let Ok(service_jid) = service_bare.parse::<Jid>() {
            for retract in items.children().filter(|child| child.name() == "retract") {
                if let Some(id) = retract.attr("id") {
                    self.pubsub.retract(&service_jid, node_id, id);
                }
            }
        }

        match node_id {
            crate::microblog::MICROBLOG_NODE => {
                let Ok(service_jid) = service_bare.parse::<Jid>() else {
                    return;
                };
                let svc = self.pubsub.get_or_create(&service_jid);
                let mut node = svc.get_or_create_node(node_id.to_string());
                for item in items.children().filter(|c| c.name() == "item") {
                    if let Some(parsed) = crate::microblog::parse_item(item, &service_jid) {
                        if node.get_item(&parsed.id).is_none() {
                            node.add_item(parsed);
                        }
                    }
                }
            }
            crate::microblog::STORIES_NODE => {
                let Ok(service_jid) = service_bare.parse::<Jid>() else {
                    return;
                };
                let svc = self.pubsub.get_or_create(&service_jid);
                let mut node = svc.get_or_create_node(node_id.to_string());
                for item in items.children().filter(|c| c.name() == "item") {
                    if let Some(parsed) = crate::microblog::parse_item_for_node(
                        item,
                        &service_jid,
                        crate::microblog::STORIES_NODE,
                    ) {
                        if node.get_item(&parsed.id).is_none() {
                            node.add_item(parsed);
                        }
                    }
                }
            }
            NS_BOOKMARKS => self.ingest_bookmarks(items),
            crate::avatar::NS_AVATAR_METADATA => self.ingest_avatar_metadata(items, &service_bare),
            crate::avatar::NS_AVATAR_DATA => self.ingest_avatar_data(items, &service_bare),
            crate::pep::NS_TUNE => {
                self.ingest_pep_node(items, &service_bare, crate::pep::NS_TUNE, |item, xml| {
                    item.tune = Some(xml);
                });
            }
            crate::pep::NS_MOOD => {
                self.ingest_pep_node(items, &service_bare, crate::pep::NS_MOOD, |item, xml| {
                    item.mood = Some(xml);
                });
            }
            crate::pep::NS_ACTIVITY => {
                self.ingest_pep_node(
                    items,
                    &service_bare,
                    crate::pep::NS_ACTIVITY,
                    |item, xml| {
                        item.activity = Some(xml);
                    },
                );
            }
            crate::pep::NS_LOCATION => {
                self.ingest_pep_node(
                    items,
                    &service_bare,
                    crate::pep::NS_LOCATION,
                    |item, xml| {
                        item.location = Some(xml);
                    },
                );
            }
            crate::pep::NS_VCARD4_NODE => {
                self.ingest_pep_node(items, &service_bare, crate::pep::NS_VCARD4, |item, xml| {
                    item.vcard4 = Some(xml);
                });
            }
            crate::omemo::NS_DEVICES => self.ingest_omemo_devices(items, &service_bare),
            crate::omemo::NS_BUNDLES => self.ingest_omemo_bundles(items, &service_bare),
            crate::omemo::NS_LEGACY_DEVICES => {
                self.ingest_legacy_omemo_devices(items, &service_bare)
            }
            _ if node_id.starts_with(crate::omemo::NS_LEGACY_BUNDLES_PREFIX) => {
                self.ingest_legacy_omemo_bundles(items, &service_bare, node_id)
            }
            _ if node_id.starts_with(crate::microblog::COMMENTS_PREFIX) => {
                self.ingest_comment_items(items, &service_bare, node_id);
            }
            _ => {}
        }
    }

    fn ingest_comment_items(&self, items: &Element, service: &str, node_id: &str) {
        let Ok(service_jid) = service.parse::<Jid>() else {
            return;
        };
        let svc = self.pubsub.get_or_create(&service_jid);
        let mut node = svc.get_or_create_node(node_id.to_string());
        for item in items.children().filter(|child| child.name() == "item") {
            if let Some(mut parsed) = crate::microblog::parse_item(item, &service_jid) {
                parsed.node_id = node_id.to_string();
                if node.get_item(&parsed.id).is_none() {
                    node.add_item(parsed);
                }
            }
        }
    }

    fn ingest_pep_node<F>(&self, items: &Element, service: &str, payload_ns: &str, apply: F)
    where
        F: FnOnce(&mut RosterItem, String),
    {
        let Ok(jid) = service.parse::<Jid>() else {
            return;
        };
        let Some(item) = items.children().find(|child| child.name() == "item") else {
            return;
        };
        let Some(payload) = item.children().find(|child| child.ns() == payload_ns) else {
            return;
        };
        let xml = crate::vcard::to_xml(payload);
        let mut entry = self
            .roster
            .get(&jid)
            .unwrap_or_else(|| RosterItem::new(jid.clone()));
        apply(&mut entry, xml);
        self.roster.upsert(entry);
    }

    fn ingest_bookmarks(&self, items: &Element) {
        for item in items.children().filter(|c| c.name() == "item") {
            let Some(conference) = item.get_child("conference", NSChoice::Any) else {
                continue;
            };
            self.ingest_bookmark_conference(conference, item.attr("id"));
        }
    }

    fn ingest_bookmark_conference(&self, conference: &Element, item_id: Option<&str>) {
        let Some(room_jid) = conference
            .attr("jid")
            .or(item_id)
            .and_then(|j| j.parse::<Jid>().ok())
        else {
            return;
        };
        let name = conference.attr("name").unwrap_or_default().to_string();
        let nick = conference
            .get_child("nick", NSChoice::Any)
            .map(|nick| nick.text())
            .filter(|nick| !nick.is_empty())
            .or_else(|| conference.attr("nick").map(str::to_string))
            .unwrap_or_else(|| {
                self.connection
                    .jid()
                    .and_then(|j| j.node().map(|n| n.to_string()))
                    .unwrap_or_else(|| "me".to_string())
            });
        let autojoin = matches!(conference.attr("autojoin"), Some("true" | "1"));
        let pinned = conference
            .get_child("extensions", NSChoice::Any)
            .and_then(|extensions| extensions.get_child("pinned", crate::spaces::NS_PINNED))
            .is_some();
        self.mucs.get_or_create(&room_jid);
        self.mucs.set_bookmark(
            &room_jid.to_bare().to_string(),
            name,
            nick.clone(),
            autojoin,
            pinned,
        );
        if autojoin {
            let presence = Element::builder("presence", ns::JABBER_CLIENT)
                .attr("to", format!("{}/{}", room_jid.to_bare(), nick))
                .append(Element::builder("x", "http://jabber.org/protocol/muc"))
                .build();
            let _ = self.connection.send_stanza_now(presence);
        }
    }

    fn ingest_avatar_metadata(&self, items: &Element, jid: &str) {
        let Ok(peer) = jid.parse::<Jid>() else {
            return;
        };
        let mut should_fetch = false;
        for item in items.children().filter(|c| c.name() == "item") {
            let Some(metadata) = item.get_child("metadata", NSChoice::Any) else {
                continue;
            };
            let Some(info) = metadata.get_child("info", NSChoice::Any) else {
                continue;
            };
            let Some(id) = info.attr("id").filter(|id| !id.is_empty()) else {
                if let Some(mut existing) = self.roster.get(&peer) {
                    existing.avatar_hash = None;
                    existing.avatar_data = None;
                    self.roster.upsert(existing);
                }
                continue;
            };
            let mime = info.attr("type").unwrap_or("image/png").to_string();
            let mut existing = self
                .roster
                .get(&peer)
                .unwrap_or_else(|| RosterItem::new(peer.clone()));
            if existing.avatar_hash.as_deref() == Some(id) && existing.avatar_data.is_some() {
                continue;
            }
            existing.avatar_hash = Some(id.to_string());
            existing.avatar_mime = Some(mime);
            self.roster.upsert(existing);
            should_fetch = true;
        }
        if should_fetch {
            let iq = Element::builder("iq", ns::JABBER_CLIENT)
                .attr("type", "get")
                .attr("to", peer.to_string())
                .attr("id", format!("avatar-data-{}", Uuid::new_v4()))
                .append(
                    Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                        Element::builder("items", "http://jabber.org/protocol/pubsub")
                            .attr("node", crate::avatar::NS_AVATAR_DATA)
                            .append(
                                Element::builder("set", "http://jabber.org/protocol/rsm").append(
                                    Element::builder("max", "http://jabber.org/protocol/rsm")
                                        .append("1"),
                                ),
                            ),
                    ),
                )
                .build();
            if let Err(e) = self.connection.send_stanza_now(iq) {
                debug!("Failed to fetch avatar data for {}: {}", peer, e);
            }
        }
    }

    fn ingest_avatar_data(&self, items: &Element, jid: &str) {
        let Ok(peer) = jid.parse::<Jid>() else {
            return;
        };
        let Some(mut existing) = self.roster.get(&peer) else {
            return;
        };
        for item in items.children().filter(|c| c.name() == "item") {
            let Some(data) = item.get_child("data", NSChoice::Any) else {
                continue;
            };
            let encoded = data.text().trim().to_string();
            if encoded.is_empty() {
                continue;
            }
            if let Some(id) = item.attr("id") {
                existing.avatar_hash = Some(id.to_string());
            }
            existing.avatar_data = Some(encoded);
            self.roster.upsert(existing.clone());
            return;
        }
    }

    fn ingest_omemo_devices(&self, items: &Element, jid: &str) {
        for item in items.children().filter(|c| c.name() == "item") {
            if let Some(devices) = item.get_child("devices", NSChoice::Any) {
                let ids: Vec<u32> = devices
                    .children()
                    .filter(|c| c.name() == "device")
                    .filter_map(|d| d.attr("id")?.parse::<u32>().ok())
                    .collect();
                self.omemo.set_device_list(jid, ids.clone());
                if !jid.is_empty() && !ids.is_empty() {
                    let iq = Element::builder("iq", ns::JABBER_CLIENT)
                        .attr("type", "get")
                        .attr("to", jid.to_string())
                        .attr("id", format!("omemo-bundles-{}", Uuid::new_v4()))
                        .append(
                            Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                                Element::builder("items", "http://jabber.org/protocol/pubsub")
                                    .attr("node", crate::omemo::NS_BUNDLES)
                                    .append(
                                        Element::builder("set", "http://jabber.org/protocol/rsm")
                                            .append(
                                                Element::builder(
                                                    "max",
                                                    "http://jabber.org/protocol/rsm",
                                                )
                                                .append("100"),
                                            ),
                                    ),
                            ),
                        )
                        .build();
                    if let Err(e) = self.connection.send_stanza_now(iq) {
                        warn!("Failed to fetch OMEMO bundles for {}: {}", jid, e);
                    }
                }
            }
        }
    }

    fn ingest_omemo_bundles(&self, items: &Element, jid: &str) {
        for item in items.children().filter(|c| c.name() == "item") {
            let Some(bundle) = item.get_child("bundle", NSChoice::Any) else {
                continue;
            };
            let Some(device_id) = item.attr("id").and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            if let Some(parsed) = DeviceBundle::from_element(bundle, device_id) {
                self.omemo.set_bundle(jid, parsed);
            }
        }
    }

    fn ingest_legacy_omemo_devices(&self, items: &Element, jid: &str) {
        for item in items.children().filter(|c| c.name() == "item") {
            let Some(list) = item.get_child("list", NSChoice::Any) else {
                continue;
            };
            let xml = crate::vcard::to_xml(list);
            let Ok(parsed) = omemo_stanza::axolotl_stanza::DeviceList::parse(&xml) else {
                continue;
            };
            self.omemo
                .set_legacy_device_list(jid, parsed.devices.clone());
            for device_id in parsed.devices {
                let node = format!("{}{}", crate::omemo::NS_LEGACY_BUNDLES_PREFIX, device_id);
                let iq = Element::builder("iq", ns::JABBER_CLIENT)
                    .attr("type", "get")
                    .attr("to", jid.to_string())
                    .attr("id", format!("legacy-omemo-bundle-{}", Uuid::new_v4()))
                    .append(
                        Element::builder("pubsub", "http://jabber.org/protocol/pubsub").append(
                            Element::builder("items", "http://jabber.org/protocol/pubsub")
                                .attr("node", node)
                                .append(
                                    Element::builder("set", "http://jabber.org/protocol/rsm")
                                        .append(
                                            Element::builder(
                                                "max",
                                                "http://jabber.org/protocol/rsm",
                                            )
                                            .append("1"),
                                        ),
                                ),
                        ),
                    )
                    .build();
                if let Err(e) = self.connection.send_stanza_now(iq) {
                    warn!("Failed to fetch legacy OMEMO bundle for {}: {}", jid, e);
                }
            }
        }
    }

    fn ingest_legacy_omemo_bundles(&self, items: &Element, jid: &str, node_id: &str) {
        let Some(device_id) = node_id
            .strip_prefix(crate::omemo::NS_LEGACY_BUNDLES_PREFIX)
            .and_then(|suffix| suffix.parse::<u32>().ok())
        else {
            return;
        };
        for item in items.children().filter(|c| c.name() == "item") {
            let Some(bundle) = item.get_child("bundle", NSChoice::Any) else {
                continue;
            };
            if let Some(parsed) = crate::omemo::LegacyDeviceBundle::from_element(bundle, device_id)
            {
                self.omemo.set_legacy_bundle(jid, parsed);
            }
        }
    }

    /// Create the media offer for a conference and send `session-initiate`.
    async fn start_conference_media(&self, sid: &str) {
        let Some(session) = self.conference.session_by_sid(sid) else {
            return;
        };
        let Some(own) = self.connection.jid() else {
            return;
        };
        let offer = match self.jingle.create_conference_offer(sid).await {
            Ok(offer) => offer,
            Err(error) => {
                tracing::warn!(sid, "failed to create conference offer: {error}");
                return;
            }
        };
        let Ok(offer) = offer.parse::<Element>() else {
            tracing::warn!(sid, "media returned an invalid conference offer");
            return;
        };
        let iq = crate::conference::session_initiate_with_offer(
            &session.sfu,
            sid,
            &own,
            offer,
            session.dialect,
        );
        if let Err(error) = self.connection.send_stanza_now(iq) {
            tracing::warn!(sid, "failed to send conference session-initiate: {error}");
        }
    }

    /// Mirror conference signalling into the media backend: install the
    /// answer, add candidates, and answer every remote content.
    async fn sync_conference_media(
        &self,
        jingle: &Element,
        from: &Option<Jid>,
        events: &[crate::conference::ConferenceEvent],
    ) {
        let (Some(from), Some(own)) = (from.as_ref(), self.connection.jid()) else {
            return;
        };
        let mut buffer = Vec::new();
        if jingle.write_to(&mut buffer).is_err() {
            return;
        }
        let raw = String::from_utf8_lossy(&buffer).into_owned();
        let mut answered_candidates = std::collections::HashSet::new();

        for event in events {
            match event {
                crate::conference::ConferenceEvent::Accepted { sid } => {
                    let session = self.conference.session_by_sid(sid);
                    let result = if session.as_ref().is_some_and(|session| session.screen) {
                        self.jingle.apply_conference_screen_answer(sid, &raw).await
                    } else {
                        self.jingle.apply_conference_answer(sid, &raw).await
                    };
                    if let Err(error) = result {
                        tracing::warn!("failed to apply conference answer: {error}");
                    }
                    if let Some(session) = session {
                        for content in session.contents.values() {
                            self.jingle.set_content_slides(
                                sid,
                                &content.stream_key(),
                                content.category == crate::conference::ConferenceCategory::Slides,
                            );
                        }
                    }
                }
                crate::conference::ConferenceEvent::Candidate { sid, key, .. } => {
                    if answered_candidates.insert(format!("{sid}|{key}")) {
                        let screen = self
                            .conference
                            .session_by_sid(sid)
                            .is_some_and(|session| session.screen);
                        let result = if screen {
                            self.jingle.add_conference_screen_candidate(sid, &raw).await
                        } else {
                            self.jingle.add_conference_candidate(sid, key, &raw).await
                        };
                        if let Err(error) = result {
                            tracing::warn!("failed to add conference candidate: {error}");
                        }
                    }
                }
                crate::conference::ConferenceEvent::ContentAdded { sid, content } => {
                    if self
                        .conference
                        .session_by_sid(sid)
                        .is_some_and(|session| session.screen)
                    {
                        continue;
                    }
                    let Some(participant) = content.participant.as_ref() else {
                        continue;
                    };
                    self.jingle.set_content_slides(
                        sid,
                        &content.stream_key(),
                        content.category == crate::conference::ConferenceCategory::Slides,
                    );
                    let dialect = self
                        .conference
                        .session_by_sid(sid)
                        .map_or(crate::conference::ConferenceDialect::Galener, |session| {
                            session.dialect
                        });
                    let separate = dialect == crate::conference::ConferenceDialect::Galener;
                    match self
                        .jingle
                        .accept_remote_content(sid, &raw, &content.stream_key(), separate)
                        .await
                    {
                        Ok(accept) => {
                            let Ok(element) = accept.parse::<Element>() else {
                                continue;
                            };
                            let Some(mut content_element) = element
                                .get_child("content", crate::jingle::NS_JINGLE)
                                .cloned()
                            else {
                                continue;
                            };
                            content_element.set_attr("name", &content.name);
                            let iq = crate::conference::content_accept_iq(
                                from,
                                sid,
                                &own,
                                participant,
                                content_element.clone(),
                                dialect,
                            );
                            if let Err(error) = self.connection.send_stanza_now(iq) {
                                tracing::warn!("failed to send conference content-accept: {error}");
                            }
                        }
                        Err(error) => {
                            tracing::warn!(
                                sid,
                                content = %content.name,
                                "failed to accept conference content: {error}"
                            );
                        }
                    }
                }
                crate::conference::ConferenceEvent::ContentRemoved {
                    sid,
                    names,
                    participant,
                } => {
                    if self
                        .conference
                        .session_by_sid(sid)
                        .is_some_and(|session| session.screen)
                    {
                        continue;
                    }
                    for name in names {
                        let key = participant.as_ref().map_or_else(
                            || name.clone(),
                            |participant| format!("{}|{name}", participant.to_bare()),
                        );
                        self.jingle.set_content_slides(sid, &key, false);
                        self.jingle.remove_content(sid, &key);
                    }
                }
                crate::conference::ConferenceEvent::Ended { sid, .. } => {
                    let screen = self
                        .conference
                        .session_by_sid(sid)
                        .is_some_and(|session| session.screen);
                    if screen {
                        self.jingle.stop_conference_screen(sid);
                    } else {
                        self.jingle.stop_session_media(sid);
                    }
                    self.conference.remove_session(sid);
                }
                _ => {}
            }
        }
    }

    async fn handle_iq(&self, elem: &Element) -> Result<()> {
        let iq_type = elem.attr("type").unwrap_or("get");

        // XEP-0166: Jingle session negotiation. Conferences and 1:1 calls use
        // separate managers, so route by session id or by the SFU JID.
        if let Some(jingle) = elem.get_child("jingle", crate::jingle::NS_JINGLE) {
            let from = elem.attr("from").and_then(|from| from.parse::<Jid>().ok());
            let is_conference = jingle
                .attr("sid")
                .is_some_and(|sid| self.conference.owns_sid(sid))
                || from
                    .as_ref()
                    .is_some_and(|from| self.conference.has_sfu(from));
            let reply = if is_conference {
                let update = self.conference.handle_jingle(elem)?;
                self.sync_conference_media(jingle, &from, &update.events)
                    .await;
                update.reply
            } else {
                self.jingle.handle_jingle_iq(elem).await?
            };
            if let Some(reply) = reply {
                let _ = self.connection.send_stanza_now(reply);
            }
            return Ok(());
        }

        if let Some(request) = crate::bob::parse_request(elem) {
            if let Some(content) = crate::bob::get(&request.cid) {
                let reply = crate::bob::response(&request, &content);
                let _ = self.connection.send_stanza_now(reply);
            }
            return Ok(());
        }
        if let Some(content) = crate::bob::parse_result(elem) {
            crate::bob::register(content);
            return Ok(());
        }

        if iq_type == "result" {
            if let Some(fin) = elem.get_child("fin", NS_MAM) {
                let query_id = fin
                    .attr("queryid")
                    .or_else(|| elem.attr("id"))
                    .unwrap_or_default();
                if let Some(query) = self.mam.take(query_id) {
                    let complete = fin.attr("complete") == Some("true");
                    self.mam.set_complete(&query.target, complete);
                    let first = fin
                        .get_child("set", NS_RSM)
                        .and_then(|set| set.get_child("first", NS_RSM))
                        .map(|first| first.text().trim().to_string())
                        .filter(|first| !first.is_empty());
                    if let (Some(first), Ok(target)) = (first, query.target.parse::<Jid>()) {
                        if query.room {
                            self.mucs.get_or_create(&target).set_mam_cursor(&first);
                        } else {
                            self.chats.get_or_create(&target).set_mam_cursor(&first);
                        }
                    }
                }
                return Ok(());
            }
        }

        if matches!(iq_type, "result" | "error") {
            if let Some(id) = elem.attr("id") {
                let outcome = if iq_type == "error" {
                    Err(parse_iq_error(elem))
                } else {
                    Ok(elem.clone())
                };
                if self.iq.resolve(id, outcome) {
                    return Ok(());
                }
            }
        }

        if iq_type == "get" && elem.get_child("ping", ns::PING).is_some() {
            if let (Some(to), Some(id)) = (elem.attr("from"), elem.attr("id")) {
                let result = Element::builder("iq", ns::JABBER_CLIENT)
                    .attr("type", "result")
                    .attr("to", to.to_string())
                    .attr("id", id.to_string())
                    .build();
                let _ = self.connection.send_stanza_now(result);
            }
            return Ok(());
        }

        // Roster result
        if let Some(query) = elem.get_child("query", NS_ROSTER) {
            if iq_type == "result" || iq_type == "set" {
                for item in query.children().filter(|c| c.name() == "item") {
                    let Some(jid_str) = item.attr("jid") else {
                        continue;
                    };
                    let Ok(jid) = jid_str.parse::<Jid>() else {
                        continue;
                    };

                    let mut roster_item = self
                        .roster
                        .get(&jid)
                        .unwrap_or_else(|| RosterItem::new(jid.clone()));

                    if let Some(name) = item.attr("name") {
                        roster_item.name = Some(name.to_string());
                    }
                    if item.attr("subscription") == Some("remove") {
                        self.roster.remove(&jid);
                        continue;
                    }
                    if let Some(sub) = item.attr("subscription") {
                        roster_item.subscription = match sub {
                            "both" => SubscriptionState::Both,
                            "to" => SubscriptionState::To,
                            "from" => SubscriptionState::From,
                            _ => SubscriptionState::None,
                        };
                    }
                    let asking = item.attr("ask") == Some("subscribe");
                    roster_item.is_pending_out = asking;
                    roster_item.ask = asking.then_some(SubscriptionState::Subscribe);
                    roster_item.groups = item
                        .children()
                        .filter(|c| c.name() == "group")
                        .map(|g| g.text())
                        .collect();

                    self.roster.upsert(roster_item);
                }
            }
            if iq_type == "set" {
                if let (Some(to), Some(id)) = (elem.attr("from"), elem.attr("id")) {
                    let result = Element::builder("iq", ns::JABBER_CLIENT)
                        .attr("type", "result")
                        .attr("to", to.to_string())
                        .attr("id", id.to_string())
                        .build();
                    let _ = self.connection.send_stanza_now(result);
                }
            }
        }

        if let Some(query) = elem.get_child("query", "jabber:iq:private") {
            if let Some(storage) = query.get_child("storage", NS_BOOKMARKS_LEGACY) {
                for conference in storage.children().filter(|c| c.name() == "conference") {
                    self.ingest_bookmark_conference(conference, None);
                }
            }
        }

        // XEP-0045: muc#admin affiliation lists (used for room OMEMO recipients).
        if let Some(query) = elem.get_child("query", "http://jabber.org/protocol/muc#admin") {
            if iq_type == "result" {
                if let Some(room_jid) = elem
                    .attr("from")
                    .and_then(|from| from.parse::<Jid>().ok())
                    .map(|jid| jid.to_bare())
                {
                    let room = self.mucs.get_or_create(&room_jid);
                    let mut lists: HashMap<MucAffiliation, Vec<String>> = HashMap::new();
                    for item in query.children().filter(|child| child.name() == "item") {
                        let Some(affiliation) = item.attr("affiliation") else {
                            continue;
                        };
                        let affiliation = match affiliation {
                            "owner" => MucAffiliation::Owner,
                            "admin" => MucAffiliation::Admin,
                            "member" => MucAffiliation::Member,
                            "outcast" => MucAffiliation::Outcast,
                            _ => MucAffiliation::None,
                        };
                        if let Some(jid) = item.attr("jid") {
                            lists.entry(affiliation).or_default().push(jid.to_string());
                        }
                    }
                    for (affiliation, jids) in lists {
                        room.set_affiliation_list(affiliation, jids);
                    }
                }
            }
        }

        // vCard result (XEP-0054): name and avatar photo.
        if let Some(vcard) = elem.get_child("vCard", "vcard-temp") {
            let who = elem
                .attr("from")
                .and_then(|s| s.parse::<Jid>().ok())
                .or_else(|| {
                    // A self vCard result has no useful `from`; use our own JID.
                    self.own_jid.try_read().ok().and_then(|guard| guard.clone())
                });

            if let Some(jid) = who {
                let mut item = self
                    .roster
                    .get(&jid)
                    .unwrap_or_else(|| RosterItem::new(jid.clone()));

                if let Some(full_name) = vcard.get_child("FN", NSChoice::Any) {
                    let text = full_name.text().trim().to_string();
                    if !text.is_empty() {
                        item.name = Some(text);
                    }
                }
                if let Some(nick) = vcard.get_child("NICKNAME", NSChoice::Any) {
                    let text = nick.text().trim().to_string();
                    if !text.is_empty() && item.name.is_none() {
                        item.name = Some(text);
                    }
                }
                if let Some(photo) = vcard.get_child("PHOTO", NSChoice::Any) {
                    if let Some(binval) = photo.get_child("BINVAL", NSChoice::Any) {
                        let data = binval.text().trim().to_string();
                        if !data.is_empty() {
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
                    }
                    if let Some(mime) = photo.get_child("TYPE", NSChoice::Any) {
                        let m = mime.text().trim().to_string();
                        if !m.is_empty() {
                            item.avatar_mime = Some(m);
                        }
                    }
                }
                item.vcard = Some(crate::vcard::to_xml(vcard));

                self.roster.upsert(item);
            }
        }

        // XEP-0012: last activity.
        if iq_type == "result" {
            if let Some(last) = elem.get_child("query", "jabber:iq:last") {
                if let Some(seconds) = last
                    .attr("seconds")
                    .and_then(|value| value.parse::<u64>().ok())
                {
                    let who = elem
                        .attr("from")
                        .and_then(|from| from.parse::<Jid>().ok())
                        .or_else(|| self.own_jid.try_read().ok().and_then(|jid| jid.clone()));
                    if let Some(jid) = who {
                        let mut item = self
                            .roster
                            .get(&jid)
                            .unwrap_or_else(|| RosterItem::new(jid.clone()));
                        item.last_seen =
                            Some(chrono::Utc::now() - chrono::Duration::seconds(seconds as i64));
                        self.roster.upsert(item);
                    }
                }
            }
        }

        // PubSub items result (XEP-0060): cached feeds.
        if let Some(pubsub) = elem.get_child("pubsub", "http://jabber.org/protocol/pubsub") {
            if let Some(items) = pubsub.get_child("items", NSChoice::Any) {
                let service = elem
                    .attr("from")
                    .and_then(|f| f.parse::<Jid>().ok())
                    .map(|j| Jid::from(j.to_bare()));
                let node_id = items.attr("node").unwrap_or_default().to_string();
                let cursor = pubsub
                    .get_child("set", NS_RSM)
                    .and_then(|set| set.get_child("last", NS_RSM))
                    .map(|last| last.text().trim().to_string())
                    .filter(|last| !last.is_empty());
                if let Some(service_jid) = service.as_ref() {
                    let server = service_jid.to_bare().to_string();
                    if self.spaces.is_known_node(&server, &node_id) {
                        self.ingest_space_items(items, &server, &node_id);
                    }
                }
                self.ingest_pubsub_items(items, service.clone());
                if let Some(service_jid) = service {
                    self.pubsub.set_cursor(&service_jid, &node_id, cursor);
                }
            }
        }

        // Service discovery items (XEP-0030): probe each advertised entity.
        if let Some(query) = elem.get_child("query", "http://jabber.org/protocol/disco#items") {
            if iq_type == "result" {
                let spaces_listing = elem
                    .attr("id")
                    .is_some_and(|id| id.starts_with("spaces-items-"));
                for item in query.children().filter(|c| c.name() == "item") {
                    if spaces_listing {
                        if let Some(node) = item.attr("node").filter(|node| !node.is_empty()) {
                            let server = item
                                .attr("jid")
                                .map(str::to_string)
                                .or_else(|| elem.attr("from").map(str::to_string))
                                .unwrap_or_default();
                            if server.is_empty() {
                                continue;
                            }
                            self.request_space_info(&crate::spaces::SpaceId::new(server, node));
                            continue;
                        }
                    }
                    let Some(target) = item.attr("jid") else {
                        continue;
                    };
                    let iq = Element::builder("iq", ns::JABBER_CLIENT)
                        .attr("type", "get")
                        .attr("to", target.to_string())
                        .attr("id", format!("disco-info-{}", Uuid::new_v4()))
                        .append(Element::builder(
                            "query",
                            "http://jabber.org/protocol/disco#info",
                        ))
                        .build();
                    if let Err(e) = self.connection.send_stanza_now(iq) {
                        warn!("Failed to probe {}: {}", target, e);
                    }
                }
            }
        }

        // Service discovery info (XEP-0030): record features and find upload service.
        if let Some(query) = elem.get_child("query", "http://jabber.org/protocol/disco#info") {
            if iq_type == "result" {
                if let (Some(node), Some(from)) = (query.attr("node"), elem.attr("from")) {
                    if let Ok(from) = from.parse::<Jid>() {
                        let id = crate::spaces::SpaceId::new(from.to_bare().to_string(), node);
                        if let Some(form) = query
                            .children()
                            .find(|child| child.name() == "x" && child.ns() == "jabber:x:data")
                        {
                            if crate::form_value(form, "pubsub#type").as_deref()
                                == Some(crate::spaces::NS_SPACES)
                            {
                                let title = crate::form_value(form, "pubsub#title");
                                let description = crate::form_value(form, "pubsub#description");
                                let members = crate::form_value(form, "pubsub#num_subscribers")
                                    .and_then(|value| value.parse::<usize>().ok());
                                let locked = crate::form_value(form, "pubsub#access_model")
                                    .is_some_and(|value| value != "open");
                                self.spaces.set_metadata(
                                    &id,
                                    title,
                                    description,
                                    None,
                                    members,
                                    Some(locked),
                                );
                                let relationships = crate::pubsub::relationships::from_form(form);
                                if !relationships.is_empty() {
                                    self.spaces.set_relationships(&id, &relationships);
                                }
                                self.spaces.mark_known(id.clone());
                                self.request_space_items(&id);
                            }
                        }
                    }
                }
                let features: Vec<String> = query
                    .children()
                    .filter(|c| c.name() == "feature")
                    .filter_map(|f| f.attr("var").map(|v| v.to_string()))
                    .collect();
                let max_file_size = query
                    .get_child("x", "urn:xmpp:http:upload:0")
                    .and_then(|x| x.get_child("max-file-size", NSChoice::Any))
                    .and_then(|m| m.text().trim().parse::<u64>().ok());
                let is_upload = features
                    .iter()
                    .any(|f| f == "urn:xmpp:http:upload:0" || f == "urn:xmpp:http:upload:1")
                    || query.children().any(|c| {
                        c.name() == "identity"
                            && c.attr("category") == Some("store")
                            && c.attr("type") == Some("file")
                    });
                let from = elem.attr("from").and_then(|f| f.parse::<Jid>().ok());
                if let Some(from) = from {
                    if query.attr("node").is_none() {
                        self.disco
                            .set_service_features(&from.to_bare().to_string(), features.clone());
                    }
                    let restrictions = query
                        .children()
                        .filter(|child| child.name() == "x" && child.ns() == "jabber:x:data")
                        .find_map(crate::disco::parse_reaction_restrictions);
                    self.disco
                        .set_reaction_restrictions(&from.to_bare().to_string(), restrictions);
                    if is_upload {
                        let jid = from.to_bare().to_string();
                        info!("Discovered upload service at {}", jid);
                        self.disco
                            .set_upload_service(UploadService { jid, max_file_size });
                    }
                    if from.node().is_none()
                        && from.resource().is_none()
                        && query.attr("node").is_none()
                    {
                        let external = features.iter().any(|f| f == NS_EXTERNAL_SERVICES);
                        let csi = features.iter().any(|f| f == NS_CSI);
                        let blocking = features.iter().any(|f| f == NS_BLOCKING);
                        let bare = from.to_bare().to_string();
                        let own_domain = self
                            .own_jid
                            .read()
                            .await
                            .as_ref()
                            .map(|jid| jid.domain().to_string())
                            .or_else(|| self.connection.jid().map(|jid| jid.domain().to_string()));
                        let spaces_capable = features.iter().any(|f| f == crate::spaces::NS_SPACES);
                        let pubsub_service = features
                            .iter()
                            .any(|f| f == "http://jabber.org/protocol/pubsub#create-nodes");
                        if !self.disco.is_spaces_service_manual() {
                            if spaces_capable {
                                if self.disco.spaces_service().as_deref() != Some(bare.as_str()) {
                                    self.disco.set_spaces_service(Some(bare.clone()));
                                    self.request_spaces(&bare);
                                }
                            } else if pubsub_service && own_domain.as_deref() != Some(bare.as_str())
                            {
                                self.disco.add_pubsub_service(bare.clone());
                                if self.disco.spaces_service().is_none() {
                                    self.disco.set_spaces_service(Some(bare.clone()));
                                    self.request_spaces(&bare);
                                }
                            }
                        }
                        if own_domain.as_deref() == Some(bare.as_str()) {
                            self.disco.set_server_features(features);
                            if let Some(form) = query.get_child("x", "jabber:x:data") {
                                self.disco.set_server_info(parse_server_info(form));
                            }
                            if external {
                                self.request_external_services();
                            }
                            if csi {
                                let _ = self.connection.send_csi_state();
                            }
                            if blocking {
                                self.roster.request_blocklist();
                            }
                        }
                    }
                }
            }
        }

        // XEP-0191: authoritative blocklist.
        if iq_type == "result" {
            if let Some(list) = elem.get_child("blocklist", NS_BLOCKING) {
                let blocked: Vec<Jid> = list
                    .children()
                    .filter(|child| child.name() == "item")
                    .filter_map(|item| item.attr("jid")?.parse::<Jid>().ok())
                    .collect();
                self.roster.set_blocklist(&blocked);
            }
        }
        if iq_type == "set" {
            for (name, blocked) in [("block", true), ("unblock", false)] {
                if let Some(push) = elem.get_child(name, NS_BLOCKING) {
                    for item in push.children().filter(|child| child.name() == "item") {
                        if let Some(jid) = item.attr("jid").and_then(|jid| jid.parse::<Jid>().ok())
                        {
                            self.roster.set_blocked(&jid, blocked);
                        }
                    }
                    if let (Some(to), Some(id)) = (elem.attr("from"), elem.attr("id")) {
                        let result = Element::builder("iq", ns::JABBER_CLIENT)
                            .attr("type", "result")
                            .attr("to", to.to_string())
                            .attr("id", id.to_string())
                            .build();
                        let _ = self.connection.send_stanza_now(result);
                    }
                }
            }
        }

        // External services (XEP-0215): STUN/TURN endpoints.
        if iq_type == "result" {
            if let Some(services) = elem.get_child("services", NS_EXTERNAL_SERVICES) {
                let parsed = parse_external_services(services);
                if !parsed.is_empty() {
                    info!("Discovered {} external service(s)", parsed.len());
                }
                self.disco.set_external_services(parsed);
            }
        }

        // Push notification registration result (XEP-0357).
        if let Some(enabled) = elem.get_child("enabled", "urn:xmpp:push:0") {
            let service = enabled.attr("jid").unwrap_or_default().to_string();
            let node = enabled.attr("node").unwrap_or_default().to_string();
            self.push.set_enabled(service, node);
        }
        if elem.get_child("disabled", "urn:xmpp:push:0").is_some() {
            self.push.clear();
        }

        // HTTP file upload slot (XEP-0363).
        if let Some(slot) = elem.get_child("slot", "urn:xmpp:http:upload:0") {
            let put = slot.get_child("put", NSChoice::Any);
            let get = slot.get_child("get", NSChoice::Any);
            if let (Some(put), Some(get)) = (put, get) {
                if let Some(put_url) = put.attr("url") {
                    let headers = put
                        .children()
                        .filter(|c| c.name() == "header")
                        .map(|h| (h.attr("name").unwrap_or_default().to_string(), h.text()))
                        .collect();
                    let upload_slot = UploadSlot {
                        put_url: put_url.to_string(),
                        get_url: get.attr("url").unwrap_or_default().to_string(),
                        headers,
                    };
                    if let Some(id) = elem.attr("id") {
                        self.disco.complete_slot(id, upload_slot);
                    }
                }
            }
        }

        Ok(())
    }
}

fn chat_state_from(elem: &Element) -> Option<ChatStateNotification> {
    for (name, state) in [
        ("composing", ChatStateNotification::Composing),
        ("paused", ChatStateNotification::Paused),
        ("inactive", ChatStateNotification::Inactive),
        ("gone", ChatStateNotification::Gone),
        ("active", ChatStateNotification::Active),
    ] {
        if elem.get_child(name, NS_CHATSTATES).is_some() {
            return Some(state);
        }
    }
    None
}

fn show_from(elem: &Element) -> PresenceShow {
    match elem
        .get_child("show", NSChoice::Any)
        .map(|s| s.text())
        .unwrap_or_default()
        .as_str()
    {
        "away" => PresenceShow::Away,
        "dnd" => PresenceShow::Dnd,
        "xa" => PresenceShow::Xa,
        "chat" => PresenceShow::Online,
        "unavailable" => PresenceShow::Unavailable,
        _ => PresenceShow::Online,
    }
}

fn muc_show(show: PresenceShow) -> MucPresenceShow {
    match show {
        PresenceShow::Online => MucPresenceShow::Online,
        PresenceShow::Away => MucPresenceShow::Away,
        PresenceShow::Dnd => MucPresenceShow::Dnd,
        PresenceShow::Xa => MucPresenceShow::Xa,
        PresenceShow::Unavailable => MucPresenceShow::Unavailable,
    }
}

fn parse_server_info(form: &Element) -> ServerInfo {
    let mut info = ServerInfo::default();
    for field in form.children().filter(|child| child.name() == "field") {
        let Some(var) = field.attr("var") else {
            continue;
        };
        let values: Vec<String> = field
            .children()
            .filter(|child| child.name() == "value")
            .map(|value| value.text().trim().to_string())
            .filter(|value| !value.is_empty())
            .collect();
        match var {
            "abuse-addresses" => info.abuse = values,
            "admin-addresses" => info.admin = values,
            "feedback-addresses" => info.feedback = values,
            "sales-addresses" => info.sales = values,
            "security-addresses" => info.security = values,
            "support-addresses" => info.support = values,
            _ => {}
        }
    }
    info
}

fn message_ids(elem: &Element) -> (Option<String>, Option<String>) {
    let origin_id = elem
        .get_child("origin-id", "urn:xmpp:sid:0")
        .and_then(|id| id.attr("id"))
        .map(str::to_string);
    let stanza_id = elem
        .get_child("stanza-id", "urn:xmpp:sid:0")
        .and_then(|id| id.attr("id"))
        .map(str::to_string);
    let origin_id = origin_id.or_else(|| {
        if stanza_id.is_none() {
            elem.attr("id").map(str::to_string)
        } else {
            None
        }
    });
    (origin_id, stanza_id)
}

/// XEP-0380 Explicit Message Encryption.
fn parse_encryption(elem: &Element) -> Option<crate::types::EncryptionInfo> {
    let eme = elem.get_child("encryption", "urn:xmpp:eme:0")?;
    let namespace = eme.attr("namespace")?.trim().to_string();
    if namespace.is_empty() {
        return None;
    }
    Some(crate::types::EncryptionInfo {
        namespace,
        name: eme
            .attr("name")
            .map(str::to_string)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Encrypted".to_string()),
        fingerprint: None,
    })
}

fn message_hints(elem: &Element) -> MessageHints {
    MessageHints {
        store: elem.get_child("store", NS_HINTS).is_some(),
        no_store: elem.get_child("no-store", NS_HINTS).is_some(),
        no_copy: elem.get_child("no-copy", NS_HINTS).is_some(),
        markable: elem.get_child("markable", NS_CHAT_MARKERS).is_some(),
    }
}

fn parse_forward_info(elem: &Element) -> Option<ForwardInfo> {
    let forwarded = elem.get_child("forwarded", NS_FORWARD)?;
    let inner = forwarded.get_child("message", NSChoice::Any)?;
    let original_jid = inner
        .attr("from")
        .and_then(|from| from.parse::<Jid>().ok())?;
    let original_timestamp = forwarded
        .get_child("delay", "urn:xmpp:delay")
        .and_then(|delay| delay.attr("stamp"))
        .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
        .map(|stamp| stamp.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);
    Some(ForwardInfo {
        original_jid,
        original_timestamp,
        original_id: inner.attr("id").unwrap_or_default().to_string(),
    })
}

fn parse_mentions(elem: &Element) -> Vec<String> {
    elem.children()
        .filter(|child| child.name() == "reference")
        .filter(|child| child.attr("type") == Some("mention"))
        .filter_map(|child| child.attr("uri"))
        .map(str::to_string)
        .collect()
}

fn parse_story_ref(elem: &Element) -> Option<crate::microblog::StoryRef> {
    if let Some(reference) = elem.children().find(|child| {
        child.name() == "reference"
            && child.ns() == "urn:xmpp:reference:0"
            && child.attr("type") == Some("data")
    }) {
        if let Some(uri) = reference.attr("uri") {
            if let Some(story) = crate::microblog::parse_story_uri(uri) {
                return Some(story);
            }
        }
        if let Some(media) = reference.get_child("media-sharing", "urn:xmpp:sims:1") {
            if let Some(uri) = media
                .get_child("sources", "urn:xmpp:sims:1")
                .and_then(|sources| sources.get_child("reference", "urn:xmpp:reference:0"))
                .and_then(|reference| reference.attr("uri"))
            {
                if let Some(story) = crate::microblog::parse_story_uri(uri) {
                    return Some(story);
                }
            }
        }
    }
    elem.get_child("x", NS_OOB)
        .and_then(|oob| oob.get_child("url", NS_OOB))
        .map(Element::text)
        .and_then(|uri| crate::microblog::parse_story_uri(&uri))
}

fn parse_media(elem: &Element) -> Vec<MediaAttachment> {
    let oob = elem.get_child("x", NS_OOB);
    let file = elem.get_child("file", NS_FILE_METADATA);
    let url = oob
        .and_then(|x| x.get_child("url", NS_OOB))
        .map(Element::text)
        .filter(|url| !url.trim().is_empty())
        .or_else(|| {
            file.and_then(|f| f.get_child("url", NS_FILE_METADATA))
                .map(Element::text)
                .filter(|url| !url.trim().is_empty())
        });
    let Some(url) = url else {
        return Vec::new();
    };
    let name = file
        .and_then(|f| f.get_child("name", NS_FILE_METADATA))
        .map(Element::text)
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            oob.and_then(|x| x.get_child("desc", NS_OOB))
                .map(Element::text)
                .filter(|desc| !desc.trim().is_empty())
        });
    let mime = file
        .and_then(|f| f.get_child("media-type", NS_FILE_METADATA))
        .map(Element::text)
        .filter(|mime| !mime.trim().is_empty())
        .unwrap_or_else(|| guess_mime(name.as_deref().unwrap_or(&url)).to_string());
    let size = file
        .and_then(|f| f.get_child("size", NS_FILE_METADATA))
        .and_then(|size| size.text().trim().parse::<u64>().ok());
    let dimensions = file.and_then(|f| f.get_child("dimensions", NS_FILE_METADATA));
    let width = dimensions
        .and_then(|d| d.get_child("width", NS_FILE_METADATA))
        .or_else(|| file.and_then(|f| f.get_child("width", NS_FILE_METADATA)))
        .and_then(|w| w.text().trim().parse::<u32>().ok());
    let height = dimensions
        .and_then(|d| d.get_child("height", NS_FILE_METADATA))
        .or_else(|| file.and_then(|f| f.get_child("height", NS_FILE_METADATA)))
        .and_then(|h| h.text().trim().parse::<u32>().ok());
    let duration = file
        .and_then(|f| f.get_child("duration", NS_FILE_METADATA))
        .and_then(|d| d.text().trim().parse::<f64>().ok());
    let thumbnail_url = file
        .and_then(|f| f.get_child("thumbnail", NS_FILE_METADATA))
        .and_then(|t| t.get_child("uri", NS_FILE_METADATA))
        .map(Element::text)
        .filter(|uri| !uri.trim().is_empty());
    let blur_hash = file
        .and_then(|f| f.get_child("blurhash", NS_FILE_METADATA))
        .map(Element::text)
        .filter(|hash| !hash.trim().is_empty());

    vec![MediaAttachment {
        id: Uuid::new_v4().to_string(),
        url,
        media_type: MediaType::from_mime(&mime),
        mime: (!mime.is_empty()).then(|| mime.clone()),
        name,
        size,
        width,
        height,
        duration,
        thumbnail_url,
        blur_hash,
    }]
}

fn guess_mime(name: &str) -> &'static str {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        _ => "application/octet-stream",
    }
}

fn parse_external_services(services: &Element) -> Vec<ExternalService> {
    services
        .children()
        .filter(|child| child.name() == "service")
        .filter_map(|service| {
            let kind = service.attr("type")?.to_string();
            let host = service.attr("host")?.to_string();
            let port = service.attr("port")?.parse::<u16>().ok()?;
            Some(ExternalService {
                kind,
                host,
                port,
                transport: service.attr("transport").map(|value| value.to_string()),
                username: service.attr("username").map(|value| value.to_string()),
                password: service.attr("password").map(|value| value.to_string()),
                expires: service.attr("expires").map(|value| value.to_string()),
                restricted: service.attr("restricted") == Some("true"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_receipt_request() {
        let xml = r#"<message xmlns="jabber:client" id="m1" from="a@b" type="chat"><body>hi</body><request xmlns="urn:xmpp:receipts"/></message>"#;
        let elem = Element::from_str(xml).unwrap();
        assert_eq!(elem.name(), "message");
        assert!(elem.get_child("request", NS_RECEIPTS).is_some());
        assert_eq!(elem.attr("id"), Some("m1"));
    }

    #[test]
    fn parses_oob_attachment_without_body() {
        let xml = r#"<message xmlns="jabber:client" from="a@b" type="chat"><x xmlns="jabber:x:oob"><url>https://files.example/cat.png</url><desc>cat.png</desc></x></message>"#;
        let elem = Element::from_str(xml).unwrap();
        let media = parse_media(&elem);
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].url, "https://files.example/cat.png");
        assert_eq!(media[0].name.as_deref(), Some("cat.png"));
        assert_eq!(media[0].media_type, MediaType::Image);
    }

    #[test]
    fn parses_file_metadata() {
        let xml = r#"<message xmlns="jabber:client" from="a@b" type="chat"><body>https://files.example/clip.mp4</body><x xmlns="jabber:x:oob"><url>https://files.example/clip.mp4</url></x><file xmlns="urn:xmpp:file:metadata:0"><name>clip.mp4</name><size>12345</size><media-type>video/mp4</media-type><dimensions><width>1280</width><height>720</height></dimensions></file></message>"#;
        let elem = Element::from_str(xml).unwrap();
        let media = parse_media(&elem);
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].media_type, MediaType::Video);
        assert_eq!(media[0].size, Some(12345));
        assert_eq!(media[0].width, Some(1280));
        assert_eq!(media[0].height, Some(720));
    }

    #[test]
    fn parses_carbon_wrapped_message() {
        let xml = r#"<message xmlns="jabber:client" from="me@home/phone" to="me@home"><sent xmlns="urn:xmpp:carbons:2"><forwarded xmlns="urn:xmpp:forward:0"><message xmlns="jabber:client" from="me@home/phone" to="friend@x" type="chat"><body>hi</body></message></forwarded></sent></message>"#;
        let elem = Element::from_str(xml).unwrap();
        let carbon = elem.get_child("sent", "urn:xmpp:carbons:2").unwrap();
        let inner = carbon
            .get_child("forwarded", "urn:xmpp:forward:0")
            .and_then(|f| f.get_child("message", NSChoice::Any))
            .unwrap();
        assert_eq!(
            inner.get_child("body", NSChoice::Any).map(|b| b.text()),
            Some("hi".to_string())
        );
    }

    #[test]
    fn parses_reactions_and_retraction() {
        let xml = r#"<message xmlns="jabber:client" from="a@b" type="chat"><reactions xmlns="urn:xmpp:reactions:0" id="m1"><reaction>👍</reaction></reactions></message>"#;
        let elem = Element::from_str(xml).unwrap();
        let r = elem.get_child("reactions", "urn:xmpp:reactions:0").unwrap();
        assert_eq!(r.attr("id"), Some("m1"));
        let emojis: Vec<String> = r
            .children()
            .filter(|c| c.name() == "reaction")
            .map(|c| c.text())
            .collect();
        assert_eq!(emojis, vec!["👍".to_string()]);

        let xml = r#"<message xmlns="jabber:client" from="a@b"><retract xmlns="urn:xmpp:message-retract:0" id="m2"/></message>"#;
        let elem = Element::from_str(xml).unwrap();
        assert_eq!(
            elem.get_child("retract", "urn:xmpp:message-retract:0")
                .and_then(|r| r.attr("id")),
            Some("m2")
        );
    }

    #[test]
    fn parses_reply_and_correction() {
        let xml = r#"<message xmlns="jabber:client" from="a@b" type="chat"><body>new</body><reply xmlns="urn:xmpp:reply:0" id="orig" to="a@b"/></message>"#;
        let elem = Element::from_str(xml).unwrap();
        assert_eq!(
            elem.get_child("reply", "urn:xmpp:reply:0")
                .and_then(|r| r.attr("id")),
            Some("orig")
        );

        let xml = r#"<message xmlns="jabber:client" from="a@b" type="chat"><body>fixed</body><replace xmlns="urn:xmpp:message-correct:0" id="m3"/></message>"#;
        let elem = Element::from_str(xml).unwrap();
        assert_eq!(
            elem.get_child("replace", "urn:xmpp:message-correct:0")
                .and_then(|r| r.attr("id")),
            Some("m3")
        );
    }

    #[test]
    fn parses_received_receipt() {
        let xml = r#"<message xmlns="jabber:client" from="a@b"><received xmlns="urn:xmpp:receipts" id="m7"/></message>"#;
        let elem = Element::from_str(xml).unwrap();
        let id = elem
            .get_child("received", NS_RECEIPTS)
            .and_then(|c| c.attr("id"));
        assert_eq!(id, Some("m7"));
    }

    #[test]
    fn parses_chat_state() {
        let xml = r#"<message xmlns="jabber:client" from="a@b" type="chat"><composing xmlns="http://jabber.org/protocol/chatstates"/></message>"#;
        let elem = Element::from_str(xml).unwrap();
        assert_eq!(
            chat_state_from(&elem),
            Some(ChatStateNotification::Composing)
        );
    }

    #[test]
    fn parses_roster_result() {
        let xml = r#"<iq xmlns="jabber:client" type="result" id="r1"><query xmlns="jabber:iq:roster"><item jid="alice@example.com" name="Alice" subscription="both"><group>Friends</group></item><item jid="bob@example.com"/></query></iq>"#;
        let elem = Element::from_str(xml).unwrap();
        let query = elem.get_child("query", NS_ROSTER).unwrap();
        let items: Vec<_> = query.children().filter(|c| c.name() == "item").collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].attr("jid"), Some("alice@example.com"));
        let groups: Vec<String> = items[0]
            .children()
            .filter(|c| c.name() == "group")
            .map(|g| g.text())
            .collect();
        assert_eq!(groups, vec!["Friends".to_string()]);
    }

    #[test]
    fn parses_vcard_result() {
        let xml = r#"<iq xmlns="jabber:client" type="result" id="v1" from="alice@example.com"><vCard xmlns="vcard-temp"><FN>Alice</FN><NICKNAME>ali</NICKNAME><PHOTO><TYPE>image/png</TYPE><BINVAL>aGVsbG8=</BINVAL></PHOTO></vCard></iq>"#;
        let elem = Element::from_str(xml).unwrap();
        let vcard = elem.get_child("vCard", "vcard-temp").unwrap();
        assert_eq!(
            vcard.get_child("FN", NSChoice::Any).map(|e| e.text()),
            Some("Alice".to_string())
        );
        let photo = vcard.get_child("PHOTO", NSChoice::Any).unwrap();
        assert_eq!(
            photo.get_child("TYPE", NSChoice::Any).map(|e| e.text()),
            Some("image/png".to_string())
        );
        assert_eq!(
            photo.get_child("BINVAL", NSChoice::Any).map(|e| e.text()),
            Some("aGVsbG8=".to_string())
        );
    }

    #[test]
    fn parses_upload_slot() {
        let xml = r#"<iq xmlns="jabber:client" type="result" id="upload-1" from="upload.example.com"><slot xmlns="urn:xmpp:http:upload:0"><put url="https://upload.example.com/put/abc"><header name="Authorization">Bearer x</header></put><get url="https://upload.example.com/get/abc"/></slot></iq>"#;
        let elem = Element::from_str(xml).unwrap();
        let slot = elem.get_child("slot", "urn:xmpp:http:upload:0").unwrap();
        let put = slot.get_child("put", NSChoice::Any).unwrap();
        assert_eq!(put.attr("url"), Some("https://upload.example.com/put/abc"));
        let header = put.children().find(|c| c.name() == "header").unwrap();
        assert_eq!(header.attr("name"), Some("Authorization"));
        assert_eq!(header.text(), "Bearer x");
        let get = slot.get_child("get", NSChoice::Any).unwrap();
        assert_eq!(get.attr("url"), Some("https://upload.example.com/get/abc"));
    }

    #[test]
    fn parses_disco_info_upload_feature() {
        let xml = r#"<iq xmlns="jabber:client" type="result" id="d1" from="upload.example.com"><query xmlns="http://jabber.org/protocol/disco#info"><identity category="store" type="file" name="Upload"/><feature var="urn:xmpp:http:upload:0"/><x xmlns="urn:xmpp:http:upload:0"><max-file-size>10485760</max-file-size></x></query></iq>"#;
        let elem = Element::from_str(xml).unwrap();
        let query = elem
            .get_child("query", "http://jabber.org/protocol/disco#info")
            .unwrap();
        let features: Vec<String> = query
            .children()
            .filter(|c| c.name() == "feature")
            .filter_map(|f| f.attr("var").map(|v| v.to_string()))
            .collect();
        assert!(features.iter().any(|f| f == "urn:xmpp:http:upload:0"));
        let max = query
            .get_child("x", "urn:xmpp:http:upload:0")
            .and_then(|x| x.get_child("max-file-size", NSChoice::Any))
            .and_then(|m| m.text().trim().parse::<u64>().ok());
        assert_eq!(max, Some(10_485_760));
    }

    #[test]
    fn parses_omemo_pep_items() {
        let xml = r#"<iq xmlns="jabber:client" type="result" id="d1" from="alice@example.com"><pubsub xmlns="http://jabber.org/protocol/pubsub"><items node="urn:xmpp:omemo:2:devices"><item id="current"><devices xmlns="urn:xmpp:omemo:2"><device id="111"/><device id="222"/></devices></item></items></pubsub></iq>"#;
        let elem = Element::from_str(xml).unwrap();
        let items = elem
            .get_child("pubsub", "http://jabber.org/protocol/pubsub")
            .and_then(|p| p.get_child("items", NSChoice::Any))
            .unwrap();
        let devices = items
            .children()
            .find(|c| c.name() == "item")
            .and_then(|i| i.get_child("devices", NSChoice::Any))
            .unwrap();
        let ids: Vec<u32> = devices
            .children()
            .filter(|c| c.name() == "device")
            .filter_map(|d| d.attr("id")?.parse().ok())
            .collect();
        assert_eq!(ids, vec![111, 222]);

        let local = crate::omemo::LocalDevice::generate(7, 2);
        let bundle_el = local.bundle().to_element();
        let item = Element::builder("item", "http://jabber.org/protocol/pubsub")
            .attr("id", "7")
            .append(bundle_el.clone())
            .build();
        let parsed = crate::omemo::DeviceBundle::from_element(&bundle_el, 7).unwrap();
        assert_eq!(item.attr("id"), Some("7"));
        assert!(parsed.verify());
        assert_eq!(parsed.prekeys.len(), 2);
    }

    #[test]
    fn parses_stream_management_nonzas() {
        let xml = r#"<enabled xmlns="urn:xmpp:sm:3" id="sess-1" resume="true" max="300"/>"#;
        let elem = Element::from_str(xml).unwrap();
        assert_eq!(elem.ns(), "urn:xmpp:sm:3");
        assert_eq!(elem.name(), "enabled");
        assert_eq!(elem.attr("id"), Some("sess-1"));
        assert_eq!(elem.attr("resume"), Some("true"));

        let xml = r#"<a xmlns="urn:xmpp:sm:3" h="42"/>"#;
        let elem = Element::from_str(xml).unwrap();
        assert_eq!(elem.attr("h").and_then(|h| h.parse::<u64>().ok()), Some(42));

        let xml = r#"<r xmlns="urn:xmpp:sm:3"/>"#;
        assert_eq!(Element::from_str(xml).unwrap().name(), "r");
    }

    #[test]
    fn parses_push_registration_result() {
        let xml = r#"<iq xmlns="jabber:client" type="result" id="p1"><enabled xmlns="urn:xmpp:push:0" jid="push.example.com" node="posts"/></iq>"#;
        let elem = Element::from_str(xml).unwrap();
        let enabled = elem.get_child("enabled", "urn:xmpp:push:0").unwrap();
        assert_eq!(enabled.attr("jid"), Some("push.example.com"));
        assert_eq!(enabled.attr("node"), Some("posts"));

        let xml = r#"<iq xmlns="jabber:client" type="result" id="p2"><disabled xmlns="urn:xmpp:push:0" jid="push.example.com"/></iq>"#;
        let elem = Element::from_str(xml).unwrap();
        assert!(elem.get_child("disabled", "urn:xmpp:push:0").is_some());
    }

    #[test]
    fn parses_groupchat_subject() {
        let xml = r#"<message xmlns="jabber:client" from="room@conf/alice" type="groupchat"><subject>Weekly sync</subject></message>"#;
        let elem = Element::from_str(xml).unwrap();
        assert_eq!(
            elem.get_child("subject", NSChoice::Any).map(|e| e.text()),
            Some("Weekly sync".to_string())
        );
    }

    #[test]
    fn parses_direct_muc_invite() {
        let xml = r#"<message xmlns="jabber:client" from="alice@example.com" to="me@example.com"><x xmlns="jabber:x:conference" jid="room@conf.example.com" reason="Join us"/></message>"#;
        let elem = Element::from_str(xml).unwrap();
        let invite = elem.get_child("x", "jabber:x:conference").unwrap();
        assert_eq!(invite.attr("jid"), Some("room@conf.example.com"));
        assert_eq!(invite.attr("reason"), Some("Join us"));
    }

    #[test]
    fn parses_muc_presence() {
        let xml = r#"<presence xmlns="jabber:client" from="room@conf/nick" type="available"><show>away</show><x xmlns="http://jabber.org/protocol/muc#user"><item affiliation="member" role="participant" jid="u@b/r"/></x></presence>"#;
        let elem = Element::from_str(xml).unwrap();
        let x = elem.get_child("x", NS_MUC_USER).unwrap();
        let item = x.get_child("item", NSChoice::Any).unwrap();
        assert_eq!(item.attr("affiliation"), Some("member"));
        assert_eq!(item.attr("role"), Some("participant"));
    }

    #[test]
    fn parses_external_services() {
        let xml = r#"<services xmlns="urn:xmpp:extdisco:2"><service type="stun" host="stun.example.org" port="3478"/><service type="turn" host="turn.example.org" port="5349" transport="tcp" username="u" password="p" restricted="true"/></services>"#;
        let elem = Element::from_str(xml).unwrap();
        let services = super::parse_external_services(&elem);
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].kind, "stun");
        assert_eq!(services[0].port, 3478);
        assert_eq!(services[1].transport.as_deref(), Some("tcp"));
        assert!(services[1].restricted);
    }

    #[test]
    fn parses_stanza_and_origin_ids() {
        let xml = r#"<message xmlns="jabber:client" id="client-1"><origin-id xmlns="urn:xmpp:sid:0" id="origin-1"/><stanza-id xmlns="urn:xmpp:sid:0" by="archive@x" id="stanza-1"/></message>"#;
        let elem = Element::from_str(xml).unwrap();
        let (origin, stanza) = super::message_ids(&elem);
        assert_eq!(origin.as_deref(), Some("origin-1"));
        assert_eq!(stanza.as_deref(), Some("stanza-1"));
    }

    #[test]
    fn falls_back_to_the_message_id_without_a_stanza_id() {
        let xml = r#"<message xmlns="jabber:client" id="client-1"/>"#;
        let elem = Element::from_str(xml).unwrap();
        let (origin, stanza) = super::message_ids(&elem);
        assert_eq!(origin.as_deref(), Some("client-1"));
        assert!(stanza.is_none());
    }

    #[test]
    fn parses_message_hints() {
        let xml = r#"<message xmlns="jabber:client"><store xmlns="urn:xmpp:hints"/><no-copy xmlns="urn:xmpp:hints"/><markable xmlns="urn:xmpp:chat-markers:0"/></message>"#;
        let elem = Element::from_str(xml).unwrap();
        let hints = super::message_hints(&elem);
        assert!(hints.store);
        assert!(hints.no_copy);
        assert!(hints.markable);
        assert!(!hints.no_store);
    }

    async fn test_processor() -> EventProcessor {
        let events = Arc::new(crate::EventBus::new());
        let (connection, _) = Connection::new(crate::ConnectionConfig::default());
        let processor = EventProcessor::new(
            Arc::new(connection),
            Arc::new(Roster::new(events.clone())),
            Arc::new(ChatManager::new(events.clone())),
            Arc::new(MucManager::new(events.clone())),
            Arc::new(PubSubManager::new(events.clone())),
            Arc::new(SpaceManager::new(events.clone())),
            Arc::new(DiscoManager::new()),
            Arc::new(PushManager::new()),
            Arc::new(StreamManagement::new()),
            Arc::new(OmemoManager::new()),
            Arc::new(IqRouter::new(std::time::Duration::from_secs(1))),
            Arc::new(MamManager::new()),
        );
        *processor.own_jid.write().await = Some("alice@localhost".parse().unwrap());
        processor
    }

    #[tokio::test]
    async fn muc_admin_affiliations_are_cached() {
        let processor = test_processor().await;
        let xml = r#"<iq xmlns="jabber:client" type="result" from="room@conference.localhost" id="aff-1"><query xmlns="http://jabber.org/protocol/muc#admin"><item affiliation="member" jid="carol@localhost"/><item affiliation="owner" jid="bob@localhost"/></query></iq>"#;
        processor.handle_stanza(xml).await.unwrap();

        let room = processor
            .mucs
            .get_by_bare("room@conference.localhost")
            .expect("room");
        let jids: Vec<String> = room
            .affiliation_jids()
            .into_iter()
            .map(|jid| jid.to_string())
            .collect();
        assert!(jids.contains(&"carol@localhost".to_string()));
        assert!(jids.contains(&"bob@localhost".to_string()));
    }

    #[tokio::test]
    async fn group_omemo_messages_decrypt_with_the_sender_real_jid() {
        let processor = test_processor().await;
        let our_device = crate::omemo::LocalDevice::generate(2, 2);
        processor.omemo.set_local(our_device.clone());
        let sender = crate::omemo::OmemoManager::new();
        let sender_device = crate::omemo::LocalDevice::generate(1, 2);
        sender.set_local(sender_device.clone());
        let envelope = sender
            .encrypt_group_message(
                "hello encrypted room",
                "room@conference.localhost",
                &[
                    ("alice@localhost".to_string(), vec![our_device.bundle()]),
                    ("bob@localhost".to_string(), vec![sender_device.bundle()]),
                ],
            )
            .expect("group envelope");

        let room: Jid = "room@conference.localhost".parse().unwrap();
        let mut room_ref = processor.mucs.get_or_create(&room);
        let mut occupant = MucOccupant::new("bob".to_string());
        occupant.jid = Some("bob@localhost".parse().unwrap());
        room_ref.add_occupant(occupant);

        let xml = format!(
            r#"<message xmlns="jabber:client" from="room@conference.localhost/bob" type="groupchat" id="g1">{}</message>"#,
            crate::vcard::to_xml(&envelope.to_element())
        );
        processor
            .handle_message(&Element::from_str(&xml).unwrap(), false)
            .await
            .unwrap();

        let message = processor
            .mucs
            .get(&room)
            .expect("room")
            .messages()
            .last()
            .cloned()
            .expect("message");
        assert_eq!(message.body.as_deref(), Some("hello encrypted room"));
        assert_eq!(
            message.encryption.map(|info| info.name),
            Some("OMEMO".to_string())
        );
    }

    #[tokio::test]
    async fn explicit_message_encryption_is_kept_for_chats_and_rooms() {
        let processor = test_processor().await;
        let direct = Element::from_str(r#"<message xmlns="jabber:client" from="bob@localhost/x" to="alice@localhost" type="chat" id="c1"><body>secret</body><encryption xmlns="urn:xmpp:eme:0" namespace="urn:xmpp:omemo:2" name="OMEMO"/></message>"#).unwrap();
        processor.handle_message(&direct, false).await.unwrap();
        let chat = processor.chats.get_by_bare("bob@localhost").expect("chat");
        let message = chat.messages().last().cloned().expect("message");
        let encryption = message.encryption.expect("encryption");
        assert_eq!(encryption.namespace, "urn:xmpp:omemo:2");
        assert_eq!(encryption.name, "OMEMO");

        let group = Element::from_str(r#"<message xmlns="jabber:client" from="room@conference.localhost/alice" type="groupchat" id="c2"><body>secret room</body><encryption xmlns="urn:xmpp:eme:0" namespace="urn:xmpp:omemo:2" name="OMEMO"/></message>"#).unwrap();
        processor.handle_message(&group, false).await.unwrap();
        let room = processor
            .mucs
            .get_by_bare("room@conference.localhost")
            .expect("room");
        let message = room.messages().last().cloned().expect("message");
        assert_eq!(
            message.encryption.map(|info| info.name),
            Some("OMEMO".to_string())
        );
    }

    #[tokio::test]
    async fn threads_and_replies_are_kept_for_chats_and_rooms() {
        let processor = test_processor().await;
        let direct = Element::from_str(r#"<message xmlns="jabber:client" from="bob@localhost/x" to="alice@localhost" type="chat" id="c1"><body>hello</body><thread>thread-1</thread><reply xmlns="urn:xmpp:reply:0" id="parent-1" to="alice@localhost"/></message>"#).unwrap();
        processor.handle_message(&direct, false).await.unwrap();

        let chat = processor.chats.get_by_bare("bob@localhost").expect("chat");
        let message = chat.messages().last().cloned().expect("message");
        assert_eq!(message.thread.as_deref(), Some("thread-1"));
        assert_eq!(message.reply_to.as_deref(), Some("parent-1"));

        let group = Element::from_str(r#"<message xmlns="jabber:client" from="room@conference.localhost/alice" type="groupchat" id="c2"><body>hello room</body><thread>thread-2</thread><reply xmlns="urn:xmpp:reply:0" id="parent-2" to="room@conference.localhost"/></message>"#).unwrap();
        processor.handle_message(&group, false).await.unwrap();

        let room = processor
            .mucs
            .get_by_bare("room@conference.localhost")
            .expect("room");
        let message = room.messages().last().cloned().expect("message");
        assert_eq!(message.thread.as_deref(), Some("thread-2"));
        assert_eq!(message.reply_to.as_deref(), Some("parent-2"));
    }

    #[tokio::test]
    async fn deduplicates_live_and_archived_messages_by_stanza_id() {
        let processor = test_processor().await;
        let live = Element::from_str(
            r#"<message xmlns="jabber:client" from="bob@example.org" to="alice@localhost" type="chat" id="client-1"><body>hello</body><origin-id xmlns="urn:xmpp:sid:0" id="origin-1"/><stanza-id xmlns="urn:xmpp:sid:0" by="localhost" id="stanza-1"/><markable xmlns="urn:xmpp:chat-markers:0"/></message>"#,
        )
        .unwrap();
        processor.handle_message(&live, false).await.unwrap();

        let archived = Element::from_str(
            r#"<message xmlns="jabber:client" from="bob@example.org" to="alice@localhost" type="chat"><body>hello</body><stanza-id xmlns="urn:xmpp:sid:0" by="localhost" id="stanza-1"/></message>"#,
        )
        .unwrap();
        processor.handle_message(&archived, false).await.unwrap();

        let chat = processor.chats.get_by_bare("bob@example.org").unwrap();
        assert_eq!(chat.message_count(), 1);
        let message = chat.find_message("stanza-1").unwrap();
        assert_eq!(message.origin_id.as_deref(), Some("origin-1"));
        assert!(message.hints.markable);
    }

    #[tokio::test]
    async fn stores_attention_without_a_body() {
        let processor = test_processor().await;
        let message = Element::from_str(
            r#"<message xmlns="jabber:client" from="bob@example.org" to="alice@localhost" type="chat"><attention xmlns="urn:xmpp:attention:0"/></message>"#,
        )
        .unwrap();
        processor.handle_message(&message, false).await.unwrap();
        let chat = processor.chats.get_by_bare("bob@example.org").unwrap();
        let stored = chat.last_message().unwrap();
        assert!(stored.attention);
        assert_eq!(stored.body.as_deref(), Some("Attention"));
    }

    #[tokio::test]
    async fn stores_room_mentions() {
        let processor = test_processor().await;
        let message = Element::from_str(
            r#"<message xmlns="jabber:client" from="room@conference.localhost/bob" to="alice@localhost" type="groupchat"><body>hi @alice</body><reference xmlns="urn:xmpp:reference:0" type="mention" uri="xmpp:room@conference.localhost/alice" begin="3" end="9"/></message>"#,
        )
        .unwrap();
        processor.handle_message(&message, false).await.unwrap();
        let room = processor
            .mucs
            .get_by_bare("room@conference.localhost")
            .unwrap();
        let stored = room.last_message().unwrap();
        assert_eq!(
            stored.mentions,
            vec!["xmpp:room@conference.localhost/alice".to_string()]
        );
    }

    #[tokio::test]
    async fn stores_forwarded_message_metadata() {
        let processor = test_processor().await;
        let message = Element::from_str(
            r#"<message xmlns="jabber:client" from="bob@example.org" to="alice@localhost" type="chat" id="f1"><forwarded xmlns="urn:xmpp:forward:0"><delay xmlns="urn:xmpp:delay" stamp="2024-01-02T03:04:05Z"/><message xmlns="jabber:client" from="carol@example.org" type="chat" id="orig-1"><body>forwarded hello</body></message></forwarded></message>"#,
        )
        .unwrap();
        processor.handle_message(&message, false).await.unwrap();
        let chat = processor.chats.get_by_bare("bob@example.org").unwrap();
        let stored = chat.last_message().unwrap();
        assert_eq!(stored.body.as_deref(), Some("forwarded hello"));
        let forward = stored.forward_info.unwrap();
        assert_eq!(forward.original_jid.to_string(), "carol@example.org");
        assert_eq!(forward.original_id, "orig-1");
    }

    #[tokio::test]
    async fn stores_oob_media_without_a_body() {
        let processor = test_processor().await;
        let message = Element::from_str(
            r#"<message xmlns="jabber:client" from="bob@example.org" to="alice@localhost" type="chat"><x xmlns="jabber:x:oob"><url>https://files.example/cat.png</url><desc>cat.png</desc></x></message>"#,
        )
        .unwrap();
        processor.handle_message(&message, false).await.unwrap();
        let chat = processor.chats.get_by_bare("bob@example.org").unwrap();
        let stored = chat.last_message().unwrap();
        assert_eq!(stored.media.len(), 1);
        assert_eq!(stored.media[0].media_type, MediaType::Image);
        assert_eq!(stored.body.as_deref(), Some("cat.png"));
    }

    #[tokio::test]
    async fn attributes_carbon_direction() {
        let processor = test_processor().await;
        let sent = Element::from_str(
            r#"<message xmlns="jabber:client" from="alice@localhost" to="alice@localhost"><sent xmlns="urn:xmpp:carbons:2"><forwarded xmlns="urn:xmpp:forward:0"><message xmlns="jabber:client" from="alice@localhost/phone" to="bob@example.org" type="chat"><body>from phone</body></message></forwarded></sent></message>"#,
        )
        .unwrap();
        processor.handle_message(&sent, false).await.unwrap();

        let received = Element::from_str(
            r#"<message xmlns="jabber:client" from="alice@localhost" to="alice@localhost"><received xmlns="urn:xmpp:carbons:2"><forwarded xmlns="urn:xmpp:forward:0"><message xmlns="jabber:client" from="bob@example.org" to="alice@localhost/phone" type="chat"><body>from bob</body></message></forwarded></received></message>"#,
        )
        .unwrap();
        processor.handle_message(&received, false).await.unwrap();

        let chat = processor.chats.get_by_bare("bob@example.org").unwrap();
        assert_eq!(chat.message_count(), 2);
        assert!(chat.messages().iter().any(|m| m.is_own));
        assert!(chat.messages().iter().any(|m| !m.is_own));
        assert_eq!(chat.unread_count(), 1);
    }

    #[tokio::test]
    async fn suppresses_own_muc_echo_without_ids() {
        let processor = test_processor().await;
        let room: Jid = "room@conference.localhost".parse().unwrap();
        processor
            .mucs
            .get_or_create(&room)
            .join("alice", None)
            .unwrap();

        let first = Element::from_str(
            r#"<message xmlns="jabber:client" from="room@conference.localhost/alice" type="groupchat"><body>hi</body><origin-id xmlns="urn:xmpp:sid:0" id="m1"/></message>"#,
        )
        .unwrap();
        processor.handle_message(&first, false).await.unwrap();

        let echo = Element::from_str(
            r#"<message xmlns="jabber:client" from="room@conference.localhost/alice" type="groupchat"><body>hi</body></message>"#,
        )
        .unwrap();
        processor.handle_message(&echo, false).await.unwrap();

        let room = processor
            .mucs
            .get_by_bare("room@conference.localhost")
            .unwrap();
        assert_eq!(room.messages().len(), 1);
        assert_eq!(room.unread_count(), 0);
    }

    #[tokio::test]
    async fn vcard_photo_sets_the_advertised_hash() {
        let processor = test_processor().await;
        let xml = r#"<iq xmlns="jabber:client" type="result" id="v1" from="bob@localhost"><vCard xmlns="vcard-temp"><FN>Bob</FN><PHOTO><TYPE>image/png</TYPE><BINVAL>aGVsbG8=</BINVAL></PHOTO></vCard></iq>"#;
        processor.handle_stanza(xml).await.unwrap();

        let peer: Jid = "bob@localhost".parse().unwrap();
        let item = processor.roster.get(&peer).expect("roster item");
        assert_eq!(item.avatar_data.as_deref(), Some("aGVsbG8="));
        assert_eq!(
            item.avatar_hash.as_deref(),
            Some("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d")
        );
    }

    #[tokio::test]
    async fn microblog_retraction_removes_the_cached_item() {
        let processor = test_processor().await;
        let published = r#"<iq xmlns="jabber:client" type="result" id="feed-1" from="alice@localhost"><pubsub xmlns="http://jabber.org/protocol/pubsub"><items node="urn:xmpp:microblog:0"><item id="p1"><entry xmlns="http://www.w3.org/2005/Atom"><id>p1</id><published>2026-11-07T09:00:00Z</published><author><uri>xmpp:alice@localhost</uri></author><content>hello</content></entry></item></items></pubsub></iq>"#;
        processor.handle_stanza(published).await.unwrap();

        let retract = r#"<message xmlns="jabber:client" from="alice@localhost" type="headline"><event xmlns="http://jabber.org/protocol/pubsub#event"><items node="urn:xmpp:microblog:0"><retract id="p1"/></items></event></message>"#;
        processor.handle_stanza(retract).await.unwrap();

        let own: Jid = "alice@localhost".parse().unwrap();
        let service = processor.pubsub.get(&own).expect("pubsub service");
        let node = service
            .get_node(crate::microblog::MICROBLOG_NODE)
            .expect("microblog node");
        assert!(node.get_item("p1").is_none());
    }

    #[tokio::test]
    async fn parses_a_story_reference_from_a_chat_message() {
        let processor = test_processor().await;
        let xml = r#"<message xmlns="jabber:client" from="bob@localhost" to="alice@localhost" type="chat" id="m1"><body>Look at this</body><reference xmlns="urn:xmpp:reference:0" type="data" uri="xmpp:bob@localhost?;node=urn%3Axmpp%3Apubsub-social-feed%3Astories%3A0;item=s1"/></message>"#;
        processor.handle_stanza(xml).await.unwrap();

        let chat = processor.chats.get_by_bare("bob@localhost").expect("chat");
        let message = chat.messages().into_iter().next().expect("message");
        let reference = message.story_ref.expect("story ref");
        assert_eq!(reference.service.to_string(), "bob@localhost");
        assert_eq!(reference.item_id, "s1");
    }

    #[tokio::test]
    async fn story_items_stay_on_the_stories_node() {
        let processor = test_processor().await;
        let xml = r#"<iq xmlns="jabber:client" type="result" id="feed-1" from="alice@localhost"><pubsub xmlns="http://jabber.org/protocol/pubsub"><items node="urn:xmpp:pubsub-social-feed:stories:0"><item id="s1"><entry xmlns="http://www.w3.org/2005/Atom"><id>s1</id><published>2026-11-07T09:00:00Z</published><author><uri>xmpp:alice@localhost</uri></author><content>sunset</content><link rel="enclosure" href="https://files.example/sunset.jpg" type="image/jpeg"/></entry></item></items></pubsub></iq>"#;
        processor.handle_stanza(xml).await.unwrap();

        let own: Jid = "alice@localhost".parse().unwrap();
        let service = processor.pubsub.get(&own).expect("pubsub service");
        let stories = service
            .get_node(crate::microblog::STORIES_NODE)
            .expect("stories node");
        let items = stories.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].node_id, crate::microblog::STORIES_NODE);
        assert!(items[0].expires_at.is_some());
        assert!(service.get_node(crate::microblog::MICROBLOG_NODE).is_none());
    }

    #[tokio::test]
    async fn pubsub_result_stores_the_rsm_cursor() {
        let processor = test_processor().await;
        let xml = r#"<iq xmlns="jabber:client" type="result" id="feed-1" from="alice@localhost"><pubsub xmlns="http://jabber.org/protocol/pubsub"><items node="urn:xmpp:microblog:0"><item id="p1"><entry xmlns="http://www.w3.org/2005/Atom"><id>p1</id><published>2026-11-07T09:00:00Z</published><author><uri>xmpp:alice@localhost</uri></author><content>hello</content></entry></item></items><set xmlns="http://jabber.org/protocol/rsm"><last>cursor-1</last></set></pubsub></iq>"#;
        processor.handle_stanza(xml).await.unwrap();

        let own: Jid = "alice@localhost".parse().unwrap();
        let service = processor.pubsub.get(&own).expect("pubsub service");
        let node = service
            .get_node(crate::microblog::MICROBLOG_NODE)
            .expect("microblog node");
        assert_eq!(node.cursor().as_deref(), Some("cursor-1"));
    }

    #[tokio::test]
    async fn idle_presence_records_the_since_timestamp() {
        let processor = test_processor().await;
        let xml = r#"<presence xmlns="jabber:client" from="bob@localhost/phone"><show>away</show><idle xmlns="urn:xmpp:idle:1" since="2026-11-07T09:30:00Z"/></presence>"#;
        processor
            .handle_presence(&Element::from_str(xml).unwrap())
            .await
            .unwrap();

        let peer: Jid = "bob@localhost".parse().unwrap();
        let item = processor.roster.get(&peer).expect("roster item");
        let presence = item.best_presence().expect("presence");
        assert_eq!(
            presence.idle_since.map(|since| since.to_rfc3339()),
            Some("2026-11-07T09:30:00+00:00".to_string())
        );
    }

    #[tokio::test]
    async fn subscription_presence_keeps_the_contact() {
        let processor = test_processor().await;
        let bob: Jid = "bob@example.org".parse().unwrap();

        let subscribe = Element::from_str(
            r#"<presence xmlns="jabber:client" from="bob@example.org" type="subscribe"/>"#,
        )
        .unwrap();
        processor.handle_presence(&subscribe).await.unwrap();
        let item = processor.roster.get(&bob).unwrap();
        assert!(item.is_pending_in);

        let subscribed = Element::from_str(
            r#"<presence xmlns="jabber:client" from="bob@example.org" type="subscribed"/>"#,
        )
        .unwrap();
        processor.handle_presence(&subscribed).await.unwrap();
        assert_eq!(
            processor.roster.get(&bob).unwrap().subscription,
            SubscriptionState::To
        );

        let mut both = processor.roster.get(&bob).unwrap();
        both.subscription = SubscriptionState::Both;
        processor.roster.upsert(both);

        let unsubscribe = Element::from_str(
            r#"<presence xmlns="jabber:client" from="bob@example.org" type="unsubscribe"/>"#,
        )
        .unwrap();
        processor.handle_presence(&unsubscribe).await.unwrap();
        assert_eq!(
            processor.roster.get(&bob).unwrap().subscription,
            SubscriptionState::To
        );

        let unsubscribed = Element::from_str(
            r#"<presence xmlns="jabber:client" from="bob@example.org" type="unsubscribed"/>"#,
        )
        .unwrap();
        processor.handle_presence(&unsubscribed).await.unwrap();
        let item = processor.roster.get(&bob).expect("contact must remain");
        assert_eq!(item.subscription, SubscriptionState::None);
        assert!(!item.is_pending_out);
        assert!(!item.is_pending_in);
    }

    #[tokio::test]
    async fn roster_push_removes_only_on_explicit_remove() {
        let processor = test_processor().await;
        let bob: Jid = "bob@example.org".parse().unwrap();
        processor.roster.upsert(RosterItem::new(bob.clone()));

        let ask = Element::from_str(
            r#"<iq xmlns="jabber:client" type="set" from="alice@localhost" id="push-1"><query xmlns="jabber:iq:roster"><item jid="bob@example.org" ask="subscribe" subscription="none"/></query></iq>"#,
        )
        .unwrap();
        processor.handle_iq(&ask).await.unwrap();
        let item = processor.roster.get(&bob).unwrap();
        assert!(item.is_pending_out);
        assert_eq!(item.ask, Some(SubscriptionState::Subscribe));

        let remove = Element::from_str(
            r#"<iq xmlns="jabber:client" type="set" from="alice@localhost" id="push-2"><query xmlns="jabber:iq:roster"><item jid="bob@example.org" subscription="remove"/></query></iq>"#,
        )
        .unwrap();
        processor.handle_iq(&remove).await.unwrap();
        assert!(processor.roster.get(&bob).is_none());
    }

    #[tokio::test]
    async fn golden_chat_and_carbons_update_the_right_chat() {
        let processor = test_processor().await;
        processor
            .handle_stanza(include_str!("../tests/golden/message_chat.xml"))
            .await
            .unwrap();
        processor
            .handle_stanza(include_str!("../tests/golden/carbon_sent.xml"))
            .await
            .unwrap();
        processor
            .handle_stanza(include_str!("../tests/golden/carbon_received.xml"))
            .await
            .unwrap();

        let bob = processor.chats.get_by_bare("bob@localhost").unwrap();
        assert_eq!(bob.message_count(), 3);
        let message = bob.find_message("stanza-1").unwrap();
        assert!(message.hints.markable);
        assert_eq!(message.origin_id.as_deref(), Some("origin-1"));
        assert!(bob.messages().iter().filter(|m| m.is_own).count() == 1);
        assert_eq!(bob.unread_count(), 2);
    }

    #[tokio::test]
    async fn golden_mam_result_and_fin_are_routed() {
        let processor = test_processor().await;
        processor
            .handle_stanza(include_str!("../tests/golden/mam_result.xml"))
            .await
            .unwrap();
        let chat = processor.chats.get_by_bare("bob@localhost").unwrap();
        assert_eq!(chat.message_count(), 1);
        assert_eq!(
            chat.find_message("stanza-2").unwrap().body.as_deref(),
            Some("Archived")
        );

        processor
            .mam
            .register("mam-1", "bob@localhost".into(), false);
        processor
            .handle_stanza(include_str!("../tests/golden/mam_fin.xml"))
            .await
            .unwrap();
        assert!(processor.mam.is_complete("bob@localhost"));
    }

    #[tokio::test]
    async fn a_moderated_room_message_is_retracted_without_a_duplicate() {
        let processor = test_processor().await;
        let room: Jid = "room@conference.localhost".parse().unwrap();
        let mut room_ref = processor.mucs.get_or_create(&room);
        let mut message = MucMessage::new(
            room.to_bare().to_string(),
            "alice".to_string(),
            None,
            false,
            Some("spam".to_string()),
        );
        message.id = "m1".to_string();
        room_ref.add_message(message);

        let xml = r#"<message xmlns="jabber:client" from="room@conference.localhost/moderator" type="groupchat" id="mod-1"><apply-to xmlns="urn:xmpp:fasten:0" id="m1"><moderated xmlns="urn:xmpp:message-moderate:1"><retract/><reason>spam</reason></moderated></apply-to><body>[This message was moderated]</body></message>"#;
        processor.handle_stanza(xml).await.unwrap();

        let room = processor.mucs.get(&room).expect("room");
        let messages = room.messages();
        assert_eq!(
            messages.len(),
            1,
            "the moderation notice must not become a message"
        );
        assert!(messages[0].is_retracted);
    }

    #[tokio::test]
    async fn a_reaction_fallback_body_is_not_a_message() {
        let processor = test_processor().await;
        let peer: Jid = "bob@localhost".parse().unwrap();
        let me: Jid = "alice@localhost".parse().unwrap();
        let mut chat = processor.chats.get_or_create(&peer);
        let mut target = ChatMessage::new(
            peer.to_bare().to_string(),
            peer.clone(),
            me,
            false,
            Some("hello".to_string()),
        );
        target.id = "m1".to_string();
        chat.add_message(target);

        let xml = r#"<message xmlns="jabber:client" from="bob@localhost" type="chat"><reactions xmlns="urn:xmpp:reactions:0" id="m1"><reaction>👍</reaction></reactions><body>reacted with 👍</body><fallback xmlns="urn:xmpp:fallback:0" for="urn:xmpp:reactions:0"/></message>"#;
        processor.handle_stanza(xml).await.unwrap();

        let chat = processor.chats.get_by_bare("bob@localhost").expect("chat");
        assert_eq!(chat.message_count(), 1);
        assert_eq!(chat.find_message("m1").expect("target").reactions.len(), 1);
    }

    #[tokio::test]
    async fn golden_receipts_markers_reactions_edits_and_retractions_apply() {
        let processor = test_processor().await;
        processor
            .handle_stanza(include_str!("../tests/golden/message_chat.xml"))
            .await
            .unwrap();
        processor
            .handle_stanza(include_str!("../tests/golden/receipt_received.xml"))
            .await
            .unwrap();
        processor
            .handle_stanza(include_str!("../tests/golden/marker_displayed.xml"))
            .await
            .unwrap();
        processor
            .handle_stanza(include_str!("../tests/golden/reaction.xml"))
            .await
            .unwrap();
        processor
            .handle_stanza(include_str!("../tests/golden/correction.xml"))
            .await
            .unwrap();
        processor
            .handle_stanza(include_str!("../tests/golden/retraction.xml"))
            .await
            .unwrap();

        let chat = processor.chats.get_by_bare("bob@localhost").unwrap();
        let message = chat.find_message("origin-1").unwrap();
        assert_eq!(message.receipt, MessageReceipt::Displayed);
        assert_eq!(message.reactions.len(), 1);
        assert_eq!(message.body.as_deref(), Some("Fixed"));
        assert!(message.is_retracted);
    }

    #[tokio::test]
    async fn golden_roster_presence_and_blocking_apply() {
        let processor = test_processor().await;
        processor
            .handle_stanza(include_str!("../tests/golden/roster_push.xml"))
            .await
            .unwrap();
        processor
            .handle_stanza(include_str!("../tests/golden/presence_subscribe.xml"))
            .await
            .unwrap();
        processor
            .handle_stanza(include_str!("../tests/golden/presence_subscribed.xml"))
            .await
            .unwrap();

        let bob: Jid = "bob@localhost".parse().unwrap();
        let item = processor.roster.get(&bob).unwrap();
        assert!(item.is_pending_in);
        assert_eq!(item.subscription, SubscriptionState::To);
        assert_eq!(item.name.as_deref(), Some("Bob"));

        processor
            .handle_stanza(include_str!("../tests/golden/blocklist_result.xml"))
            .await
            .unwrap();
        assert!(processor.roster.get(&bob).unwrap().is_blocked);
    }

    #[tokio::test]
    async fn disco_info_stores_reaction_restrictions() {
        let processor = test_processor().await;
        let xml = r#"<iq xmlns="jabber:client" type="result" id="d1" from="bob@localhost/resource"><query xmlns="http://jabber.org/protocol/disco#info"><feature var="urn:xmpp:reactions:0"/><x xmlns="jabber:x:data" type="result"><field var="FORM_TYPE" type="hidden"><value>urn:xmpp:reactions:0:restrictions</value></field><field var="max_reactions_per_user"><value>1</value></field><field var="allowlist"><value>💘</value></field></x></query></iq>"#;
        processor.handle_stanza(xml).await.unwrap();
        let restrictions = processor
            .disco
            .reaction_restrictions("bob@localhost")
            .expect("restrictions stored");
        assert_eq!(restrictions.max(), Some(1));
        assert!(restrictions.allows("💘"));
        assert!(!restrictions.allows("👍"));
    }

    #[tokio::test]
    async fn disco_info_selects_a_pubsub_service_for_spaces() {
        let processor = test_processor().await;
        let xml = r#"<iq xmlns="jabber:client" type="result" id="d1" from="pubsub.localhost"><query xmlns="http://jabber.org/protocol/disco#info"><identity category="pubsub" type="service"/><feature var="http://jabber.org/protocol/pubsub#create-nodes"/><feature var="http://jabber.org/protocol/pubsub#config-node"/></query></iq>"#;
        processor.handle_stanza(xml).await.unwrap();
        assert_eq!(
            processor.disco.spaces_service().as_deref(),
            Some("pubsub.localhost")
        );
        assert_eq!(
            processor.disco.pubsub_services(),
            vec!["pubsub.localhost".to_string()]
        );
    }

    #[tokio::test]
    async fn disco_info_prefers_a_spaces_capable_service() {
        let processor = test_processor().await;
        processor
            .handle_stanza(
                r#"<iq xmlns="jabber:client" type="result" id="d1" from="pubsub.localhost"><query xmlns="http://jabber.org/protocol/disco#info"><feature var="http://jabber.org/protocol/pubsub#create-nodes"/></query></iq>"#,
            )
            .await
            .unwrap();
        processor
            .handle_stanza(
                r#"<iq xmlns="jabber:client" type="result" id="d2" from="spaces.localhost"><query xmlns="http://jabber.org/protocol/disco#info"><feature var="urn:xmpp:spaces:0"/><feature var="http://jabber.org/protocol/pubsub#create-nodes"/></query></iq>"#,
            )
            .await
            .unwrap();
        assert_eq!(
            processor.disco.spaces_service().as_deref(),
            Some("spaces.localhost")
        );
    }

    #[tokio::test]
    async fn space_items_are_only_listed_after_type_is_confirmed() {
        let processor = test_processor().await;
        processor
            .handle_stanza(
                r#"<iq xmlns="jabber:client" type="result" id="spaces-items-1" from="pubsub.localhost"><query xmlns="http://jabber.org/protocol/disco#items"><item jid="pubsub.localhost" node="dev" name="Dev"/></query></iq>"#,
            )
            .await
            .unwrap();
        assert!(processor.spaces.list().is_empty());

        processor
            .handle_stanza(
                r#"<iq xmlns="jabber:client" type="result" id="space-info-1" from="pubsub.localhost"><query xmlns="http://jabber.org/protocol/disco#info" node="dev"><x xmlns="jabber:x:data" type="result"><field var="FORM_TYPE"><value>http://jabber.org/protocol/pubsub#meta-data</value></field><field var="pubsub#type"><value>urn:xmpp:spaces:0</value></field><field var="pubsub#title"><value>Dev</value></field></x></query></iq>"#,
            )
            .await
            .unwrap();
        let spaces = processor.spaces.list();
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces[0].id.server, "pubsub.localhost");
        assert_eq!(spaces[0].id.node, "dev");
        assert_eq!(spaces[0].name, "Dev");
    }

    #[tokio::test]
    async fn space_relationships_are_parsed_from_metadata() {
        let processor = test_processor().await;
        processor
            .handle_stanza(
                r#"<iq xmlns="jabber:client" type="result" id="space-info-2" from="pubsub.localhost"><query xmlns="http://jabber.org/protocol/disco#info" node="dev"><x xmlns="jabber:x:data" type="result"><field var="FORM_TYPE"><value>http://jabber.org/protocol/pubsub#meta-data</value></field><field var="pubsub#type"><value>urn:xmpp:spaces:0</value></field><field var="pubsub#title"><value>Dev</value></field><field var="{urn:xmpp:pubsub-relationships:0}parent"><value>urn:xmpp:microblog:0</value></field><field var="{urn:xmpp:pubsub-relationships:0}link"><value>urn:xmpp:microblog:0:comments/a</value></field></x></query></iq>"#,
            )
            .await
            .unwrap();
        let spaces = processor.spaces.list();
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces[0].parent.as_deref(), Some("urn:xmpp:microblog:0"));
        assert_eq!(
            spaces[0].links,
            vec!["urn:xmpp:microblog:0:comments/a".to_string()]
        );
    }

    #[tokio::test]
    async fn caches_service_features_from_disco_info() {
        let processor = test_processor().await;
        processor
            .handle_stanza(
                r#"<iq xmlns="jabber:client" type="result" id="d3" from="pubsub.localhost"><query xmlns="http://jabber.org/protocol/disco#info"><feature var="urn:xmpp:pubsub-filter:0"/></query></iq>"#,
            )
            .await
            .unwrap();
        assert!(processor.disco.has_service_feature(
            "pubsub.localhost",
            crate::pubsub::discovery::NS_PUBSUB_FILTER
        ));
    }

    #[tokio::test]
    async fn golden_disco_extdisco_and_stream_management_apply() {
        let processor = test_processor().await;
        processor
            .handle_stanza(include_str!("../tests/golden/disco_info.xml"))
            .await
            .unwrap();
        assert!(processor.disco.has_feature("urn:xmpp:mam:2"));

        processor
            .handle_stanza(include_str!("../tests/golden/extdisco_result.xml"))
            .await
            .unwrap();
        let services = processor.disco.stun_turn_servers();
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].kind, "stun");

        processor
            .handle_stanza(include_str!("../tests/golden/sm_enabled.xml"))
            .await
            .unwrap();
        let details = processor.sm.details().unwrap();
        assert_eq!(details.id.as_deref(), Some("sm-1"));
        assert!(details.resume);
    }

    #[tokio::test]
    async fn pep_bookmarks_restore_a_pinned_room() {
        let processor = test_processor().await;
        let xml = r#"<iq xmlns="jabber:client" type="result" id="bookmarks-1"><pubsub xmlns="http://jabber.org/protocol/pubsub"><items node="urn:xmpp:bookmarks:1"><item id="room@conference.localhost"><conference xmlns="urn:xmpp:bookmarks:1" name="Room" autojoin="true"><nick>alice</nick><extensions><pinned xmlns="urn:xmpp:bookmarks-pinning:0"/></extensions></conference></item></items></pubsub></iq>"#;
        processor.handle_stanza(xml).await.unwrap();

        let room = processor
            .mucs
            .get_by_bare("room@conference.localhost")
            .expect("bookmarked room");
        assert_eq!(room.name, "Room");
        assert_eq!(room.self_nick().as_deref(), Some("alice"));
        assert!(room.is_bookmarked());
        assert!(room.is_autojoin());
        assert!(room.is_pinned());
    }

    #[tokio::test]
    async fn legacy_bookmarks_still_restore_a_room() {
        let processor = test_processor().await;
        let xml = r#"<iq xmlns="jabber:client" type="result" id="bookmarks-legacy-1"><query xmlns="jabber:iq:private"><storage xmlns="storage:bookmarks"><conference jid="old@conference.localhost" name="Old" autojoin="1"><nick>alice</nick></conference></storage></query></iq>"#;
        processor.handle_stanza(xml).await.unwrap();

        let room = processor
            .mucs
            .get_by_bare("old@conference.localhost")
            .expect("legacy bookmarked room");
        assert_eq!(room.name, "Old");
        assert_eq!(room.self_nick().as_deref(), Some("alice"));
        assert!(room.is_bookmarked());
        assert!(room.is_autojoin());
        assert!(!room.is_pinned());
    }

    #[tokio::test]
    async fn golden_muc_and_bookmarks_apply() {
        let processor = test_processor().await;
        processor
            .handle_stanza(include_str!("../tests/golden/muc_message.xml"))
            .await
            .unwrap();
        let room = processor
            .mucs
            .get_by_bare("room@conference.localhost")
            .unwrap();
        assert_eq!(room.messages().len(), 1);

        processor
            .handle_stanza(include_str!("../tests/golden/bookmarks.xml"))
            .await
            .unwrap();
        assert!(processor
            .mucs
            .get_by_bare("room@conference.localhost")
            .is_some());
        let room = processor
            .mucs
            .get_by_bare("room@conference.localhost")
            .expect("room");
        assert_eq!(room.name, "Room");
        assert_eq!(room.self_nick().as_deref(), Some("alice"));
        assert!(room.is_pinned());
    }

    #[tokio::test]
    async fn golden_pep_activity_and_caps_update_contacts() {
        let processor = test_processor().await;
        processor
            .handle_stanza(include_str!("../tests/golden/pep_tune_event.xml"))
            .await
            .unwrap();
        processor
            .handle_stanza(include_str!("../tests/golden/pep_vcard4_event.xml"))
            .await
            .unwrap();
        processor
            .handle_stanza(include_str!("../tests/golden/presence_caps.xml"))
            .await
            .unwrap();

        let bob: Jid = "bob@localhost".parse().unwrap();
        let item = processor.roster.get(&bob).unwrap();
        assert!(item.tune.as_deref().unwrap_or_default().contains("Artist"));
        assert!(item
            .vcard4
            .as_deref()
            .unwrap_or_default()
            .contains("Bob Four"));
        let caps = item.caps.unwrap();
        assert_eq!(caps.node, "https://example.org/client");
        assert_eq!(caps.ver, "abc123");
    }

    #[tokio::test]
    async fn golden_server_info_is_parsed() {
        let processor = test_processor().await;
        processor
            .handle_stanza(include_str!("../tests/golden/server_info.xml"))
            .await
            .unwrap();
        let info = processor.disco.server_info();
        assert_eq!(info.abuse, vec!["abuse@localhost".to_string()]);
        assert_eq!(info.security, vec!["security@localhost".to_string()]);
    }
}
