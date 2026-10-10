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
    ) -> Self {
        Self {
            connection,
            roster,
            chats,
            mucs,
            pubsub,
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
                self.send_initial_presence();
                self.request_roster();
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
        let from = elem.attr("from").unwrap_or_default().to_string();
        let msg_type = elem.attr("type").unwrap_or("normal").to_string();
        let from_jid: Option<Jid> = from.parse().ok();
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
            chat.add_message(msg);
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
    fn parses_muc_presence() {
        let xml = r#"<presence xmlns="jabber:client" from="room@conf/nick" type="available"><show>away</show><x xmlns="http://jabber.org/protocol/muc#user"><item affiliation="member" role="participant" jid="u@b/r"/></x></presence>"#;
        let elem = Element::from_str(xml).unwrap();
        let x = elem.get_child("x", NS_MUC_USER).unwrap();
        let item = x.get_child("item", NSChoice::Any).unwrap();
        assert_eq!(item.attr("affiliation"), Some("member"));
        assert_eq!(item.attr("role"), Some("participant"));
    }
}
