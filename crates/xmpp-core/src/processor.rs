//! XMPP Event Processor - handles incoming XMPP events and updates local state

use dioxus::prelude::{Readable, Signal, Writable};
use jid::Jid;
use minidom::{Element, NSChoice};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;
use xmpp_parsers::ns;

use crate::{
    chat::{ChatMessage, ChatStateNotification, MessageReceipt, MessageType},
    connection::ConnectionState,
    disco::{DiscoManager, UploadService, UploadSlot},
    muc::{MucAffiliation, MucMessage, MucOccupant, MucRole, PresenceShow as MucPresenceShow},
    roster::{Presence, PresenceShow, RosterItem, SubscriptionState},
    ChatManager, Connection, ConnectionEvent, MucManager, PubSubManager, Result, Roster, XmppError,
};

const NS_RECEIPTS: &str = "urn:xmpp:receipts";
const NS_CHATSTATES: &str = "http://jabber.org/protocol/chatstates";
const NS_CHAT_MARKERS: &str = "urn:xmpp:chat-markers:0";
const NS_MUC_USER: &str = "http://jabber.org/protocol/muc#user";
const NS_ROSTER: &str = "jabber:iq:roster";

/// Event processor that handles incoming XMPP events and updates local state
pub struct EventProcessor {
    connection: Arc<Connection>,
    roster: Arc<Roster>,
    chats: Arc<ChatManager>,
    mucs: Arc<MucManager>,
    pubsub: Arc<PubSubManager>,
    disco: Arc<DiscoManager>,
    own_jid: Arc<tokio::sync::RwLock<Option<Jid>>>,
    revision: Option<Signal<u64>>,
    connection_state: Option<Signal<ConnectionState>>,
}

impl EventProcessor {
    pub fn new(
        connection: Arc<Connection>,
        roster: Arc<Roster>,
        chats: Arc<ChatManager>,
        mucs: Arc<MucManager>,
        pubsub: Arc<PubSubManager>,
        disco: Arc<DiscoManager>,
    ) -> Self {
        Self {
            connection,
            roster,
            chats,
            mucs,
            pubsub,
            disco,
            own_jid: Arc::new(tokio::sync::RwLock::new(None)),
            revision: None,
            connection_state: None,
        }
    }

    /// Attach the UI signals so the processor can notify the interface.
    pub fn with_ui(
        mut self,
        revision: Signal<u64>,
        connection_state: Signal<ConnectionState>,
    ) -> Self {
        self.revision = Some(revision);
        self.connection_state = Some(connection_state);
        self
    }

    fn bump(&self) {
        if let Some(mut revision) = self.revision {
            let next = *revision.read() + 1;
            revision.set(next);
        }
    }

    fn set_connection_state(&self, state: ConnectionState) {
        if let Some(mut signal) = self.connection_state {
            signal.set(state);
        }
    }

    pub async fn start(
        self,
        mut event_rx: tokio::sync::mpsc::UnboundedReceiver<ConnectionEvent>,
    ) -> Result<()> {
        info!("Event processor started");
        while let Some(event) = event_rx.recv().await {
            if let Err(e) = self.handle_connection_event(event).await {
                warn!("Failed to handle connection event: {}", e);
            }
            // Data may have changed (state, roster, chats, MUCs) - refresh the UI.
            self.bump();
        }
        Ok(())
    }

    async fn handle_connection_event(&self, event: ConnectionEvent) -> Result<()> {
        match event {
            ConnectionEvent::Connected { jid } => {
                info!("Connected as {}", jid);
                if let Ok(parsed_jid) = jid.parse::<Jid>() {
                    *self.own_jid.write().await = Some(parsed_jid);
                }
                self.set_connection_state(ConnectionState::Connected);
                // Announce presence and fetch the roster so the client comes alive.
                self.enable_carbons();
                self.send_initial_presence();
                self.request_roster();
                self.request_own_vcard();
                self.discover_server();
            }
            ConnectionEvent::Authenticated => {
                info!("Authenticated");
                self.set_connection_state(ConnectionState::Authenticated);
            }
            ConnectionEvent::Disconnected { reason } => {
                info!("Disconnected: {}", reason);
                *self.own_jid.write().await = None;
                self.set_connection_state(ConnectionState::Disconnected);
            }
            ConnectionEvent::Reconnecting { attempt } => {
                info!("Reconnecting (attempt {})", attempt);
                self.set_connection_state(ConnectionState::Reconnecting { attempt });
            }
            ConnectionEvent::ReconnectTriggered => {
                info!("Reconnect triggered");
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
        let presence = Element::builder("presence", ns::JABBER_CLIENT).build();
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

    fn discover_server(&self) {
        let Some(own) = self.connection.jid() else {
            return;
        };
        let iq = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("to", own.domain().to_string())
            .attr("id", "disco-init")
            .append(Element::builder(
                "query",
                "http://jabber.org/protocol/disco#items",
            ))
            .build();
        if let Err(e) = self.connection.send_stanza_now(iq) {
            warn!("Failed to start service discovery: {}", e);
        }
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

        match elem.name() {
            "message" => self.handle_message(&elem).await,
            "presence" => self.handle_presence(&elem).await,
            "iq" => self.handle_iq(&elem).await,
            _ => Ok(()),
        }
    }

    async fn handle_message(&self, elem: &Element) -> Result<()> {
        // XEP-0280: message carbons wrap the real message in <sent/> or <received/>.
        if let Some(carbon) = elem
            .get_child("sent", "urn:xmpp:carbons:2")
            .or_else(|| elem.get_child("received", "urn:xmpp:carbons:2"))
        {
            if let Some(inner) = carbon
                .get_child("forwarded", "urn:xmpp:forward:0")
                .and_then(|f| f.get_child("message", NSChoice::Any))
            {
                return Box::pin(self.handle_message(inner)).await;
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
        let reply_to: Option<String> = elem
            .get_child("reply", "urn:xmpp:reply:0")
            .and_then(|r| r.attr("id"))
            .map(|s| s.to_string());

        // --- XEP-0444: Reactions ---
        if let Some(reactions) = elem.get_child("reactions", "urn:xmpp:reactions:0") {
            if let (Some(target_id), Some(peer)) = (reactions.attr("id"), from_jid.as_ref()) {
                let emojis: Vec<String> = reactions
                    .children()
                    .filter(|c| c.name() == "reaction")
                    .map(|c| c.text())
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut chat = self.chats.get_or_create(peer);
                chat.set_reactions_from(target_id, peer, &emojis);
            }
        }

        // --- XEP-0308: Last message correction ---
        if let Some(replace_id) = elem
            .get_child("replace", "urn:xmpp:message-correct:0")
            .and_then(|r| r.attr("id"))
        {
            if let (Some(peer), Some(new_body)) = (
                from_jid.as_ref(),
                elem.get_child("body", NSChoice::Any).map(|b| b.text()),
            ) {
                let mut chat = self.chats.get_or_create(peer);
                chat.apply_edit(replace_id, &new_body);
            }
        }

        // --- XEP-0424: Message retraction ---
        if let Some(retract_id) = elem
            .get_child("retract", "urn:xmpp:message-retract:0")
            .and_then(|r| r.attr("id"))
        {
            if let Some(peer) = from_jid.as_ref() {
                let mut chat = self.chats.get_or_create(peer);
                chat.retract(retract_id);
            }
        }
        let body = elem
            .get_child("body", NSChoice::Any)
            .map(|b| b.text())
            .filter(|b| !b.is_empty());

        // --- XEP-0184: Message Delivery Receipts ---
        if elem.get_child("request", NS_RECEIPTS).is_some() {
            if let (Some(to), Some(request_id)) = (from_jid.as_ref(), elem.attr("id")) {
                let receipt = Element::builder("message", ns::JABBER_CLIENT)
                    .attr("to", to.to_string())
                    .attr("id", format!("rec-{}", Uuid::new_v4()))
                    .append(Element::builder("received", NS_RECEIPTS).attr("id", request_id))
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

        let Some(body) = body else {
            return Ok(());
        };

        let is_muc = msg_type == "groupchat" || elem.get_child("x", NS_MUC_USER).is_some();

        if is_muc {
            let room_part = from.split('/').next().unwrap_or(&from);
            let nick = from.split('/').nth(1).unwrap_or("");
            let Ok(room_jid) = room_part.parse::<Jid>() else {
                return Ok(());
            };

            let mut room = self.mucs.get_or_create(&room_jid);
            let is_own = room.self_nick().as_deref() == Some(nick);
            let msg = MucMessage::new(
                room.bare_jid.clone(),
                nick.to_string(),
                from_jid.clone(),
                is_own,
                Some(body),
            );
            room.add_message(msg);
        } else if let Ok(peer) = from.parse::<Jid>() {
            let mut chat = self.chats.get_or_create(&peer);
            let mut msg = ChatMessage::new(
                peer.to_bare().to_string(),
                peer.clone(),
                peer.clone(),
                false,
                Some(body),
            );
            msg.message_type = match msg_type.as_str() {
                "chat" => MessageType::Chat,
                "groupchat" => MessageType::GroupChat,
                "headline" => MessageType::Headline,
                "error" => MessageType::Error,
                _ => MessageType::Normal,
            };
            msg.reply_to = reply_to;
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
        let Some(body) = inner
            .get_child("body", NSChoice::Any)
            .map(|b| b.text())
            .filter(|b| !b.is_empty())
        else {
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
        let id = if archive_id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            archive_id
        };

        let timestamp = forwarded
            .get_child("delay", "urn:xmpp:delay")
            .and_then(|d| d.attr("stamp"))
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        let mut chat = self.chats.get_or_create(&peer);
        if chat.get_message(&id).is_some() {
            return Ok(());
        }

        let mut msg = ChatMessage::new(
            peer.to_bare().to_string(),
            peer.clone(),
            peer.clone(),
            is_own,
            Some(body),
        );
        msg.id = id;
        msg.timestamp = timestamp;
        chat.add_message(msg);
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
                                "owner" => MucRole::Owner,
                                "admin" => MucRole::Admin,
                                "visitor" => MucRole::Visitor,
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
                let _ = self.roster.approve_subscription(&from_jid);
            }
            "subscribed" => {
                item.subscription = SubscriptionState::To;
                self.roster.upsert(item);
            }
            "unsubscribe" | "unsubscribed" => {
                self.roster.remove(&from_jid);
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
        if node_id != crate::microblog::MICROBLOG_NODE {
            return;
        }
        let Some(service_jid) =
            service.or_else(|| self.connection.jid().map(|j| Jid::from(j.to_bare())))
        else {
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

    async fn handle_iq(&self, elem: &Element) -> Result<()> {
        let iq_type = elem.attr("type").unwrap_or("get");

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
                    if let Some(sub) = item.attr("subscription") {
                        roster_item.subscription = match sub {
                            "both" => SubscriptionState::Both,
                            "to" => SubscriptionState::To,
                            "from" => SubscriptionState::From,
                            "remove" => SubscriptionState::Remove,
                            _ => SubscriptionState::None,
                        };
                    }
                    roster_item.groups = item
                        .children()
                        .filter(|c| c.name() == "group")
                        .map(|g| g.text())
                        .collect();

                    self.roster.upsert(roster_item);
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

                self.roster.upsert(item);
            }
        }

        // PubSub items result (XEP-0060): cached feeds.
        if let Some(pubsub) = elem.get_child("pubsub", "http://jabber.org/protocol/pubsub") {
            if let Some(items) = pubsub.get_child("items", NSChoice::Any) {
                let service = elem
                    .attr("from")
                    .and_then(|f| f.parse::<Jid>().ok())
                    .map(|j| Jid::from(j.to_bare()));
                self.ingest_pubsub_items(items, service);
            }
        }

        // Service discovery items (XEP-0030): probe each advertised entity.
        if let Some(query) = elem.get_child("query", "http://jabber.org/protocol/disco#items") {
            if iq_type == "result" {
                for item in query.children().filter(|c| c.name() == "item") {
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
                    if is_upload {
                        let jid = from.to_bare().to_string();
                        info!("Discovered upload service at {}", jid);
                        self.disco
                            .set_upload_service(UploadService { jid, max_file_size });
                    }
                    if from.node().is_none() && from.resource().is_none() {
                        self.disco.set_server_features(features);
                    }
                }
            }
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
    fn parses_muc_presence() {
        let xml = r#"<presence xmlns="jabber:client" from="room@conf/nick" type="available"><show>away</show><x xmlns="http://jabber.org/protocol/muc#user"><item affiliation="member" role="participant" jid="u@b/r"/></x></presence>"#;
        let elem = Element::from_str(xml).unwrap();
        let x = elem.get_child("x", NS_MUC_USER).unwrap();
        let item = x.get_child("item", NSChoice::Any).unwrap();
        assert_eq!(item.attr("affiliation"), Some("member"));
        assert_eq!(item.attr("role"), Some("participant"));
    }
}
