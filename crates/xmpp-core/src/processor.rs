//! XMPP Event Processor - handles incoming XMPP events and updates local state

use jid::Jid;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::{
    chat::{ChatMessage, MessageType},
    muc::{MucAffiliation, MucMessage, MucOccupant, MucRole, PresenceShow as MucPresenceShow},
    roster::{Presence, PresenceShow, SubscriptionState},
    ChatManager, Connection, ConnectionEvent, MucManager, PubSubManager, Result, Roster, XmppError,
};

/// Event processor that handles incoming XMPP events and updates local state
pub struct EventProcessor {
    connection: Arc<Connection>,
    roster: Arc<Roster>,
    chats: Arc<ChatManager>,
    mucs: Arc<MucManager>,
    pubsub: Arc<PubSubManager>,
    own_jid: Arc<tokio::sync::RwLock<Option<Jid>>>,
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
            }
            ConnectionEvent::Authenticated => {
                info!("Authenticated");
            }
            ConnectionEvent::Disconnected { reason } => {
                info!("Disconnected: {}", reason);
                *self.own_jid.write().await = None;
            }
            ConnectionEvent::Reconnecting { attempt } => {
                info!("Reconnecting (attempt {})", attempt);
            }
            ConnectionEvent::ReconnectTriggered => {
                info!("Reconnect triggered");
            }
            ConnectionEvent::ReconnectFailed => {
                warn!("Reconnection failed");
            }
            ConnectionEvent::StanzaReceived(xml) => {
                debug!("Received stanza: {}", xml);
                if let Err(e) = self.handle_stanza(&xml).await {
                    warn!("Failed to handle stanza: {}", e);
                }
            }
            ConnectionEvent::Error(err) => {
                warn!("Connection error: {}", err);
            }
        }
        Ok(())
    }

    async fn handle_stanza(&self, xml: &str) -> Result<()> {
        // Simple XML parsing - extract element name and attributes
        let element_name = extract_element_name(xml)?;

        match element_name.as_str() {
            "message" => self.handle_message(xml).await,
            "presence" => self.handle_presence(xml).await,
            "iq" => self.handle_iq(xml).await,
            _ => Ok(()),
        }
    }

    async fn handle_message(&self, xml: &str) -> Result<()> {
        // Extract from, type, id, body
        let from = extract_attr(xml, "from").unwrap_or_default();
        let msg_type = extract_attr(xml, "type").unwrap_or_else(|| "normal".to_string());
        let body = extract_child_text(xml, "body").unwrap_or_default();

        if body.is_empty() {
            return Ok(());
        }

        // Check if it's a MUC message
        let is_muc = xml.contains("xmlns=\"http://jabber.org/protocol/muc#user\"");

        if is_muc {
            // MUC message
            let muc_jid = from.split('/').next().unwrap_or(&from);
            let nick = from.split('/').nth(1).unwrap_or("");

            let muc_msg = MucMessage::new(
                muc_jid.to_string(),
                nick.to_string(),
                None,
                false,
                Some(body),
            );

            let mut room = self.mucs.get_or_create(&muc_jid.parse::<Jid>().unwrap());
            room.add_message(muc_msg);
        } else {
            // 1:1 chat message
            let mut chat = self.chats.get_or_create(&from.parse::<Jid>().unwrap());

            let mut chat_msg = ChatMessage::new(
                from.to_string(),
                from.parse().unwrap(),
                from.parse().unwrap(),
                false,
                Some(body),
            );
            chat_msg.message_type = match msg_type.as_str() {
                "chat" => MessageType::Chat,
                "groupchat" => MessageType::GroupChat,
                "headline" => MessageType::Headline,
                "error" => MessageType::Error,
                _ => MessageType::Normal,
            };
            chat.add_message(chat_msg);
        }

        Ok(())
    }

    async fn handle_presence(&self, xml: &str) -> Result<()> {
        use crate::muc::MucPresence;
        use crate::roster::SubscriptionState;

        let from = extract_attr(xml, "from").unwrap_or_default();
        let ptype = extract_attr(xml, "type").unwrap_or_else(|| "available".to_string());

        // Get show and status
        let show_str = extract_child_text(xml, "show").unwrap_or_default();
        let show = match show_str.as_str() {
            "away" => PresenceShow::Away,
            "dnd" => PresenceShow::Dnd,
            "xa" => PresenceShow::Xa,
            "chat" => PresenceShow::Online,
            _ => PresenceShow::Online,
        };

        let status = extract_child_text(xml, "status");

        // Check if it's a MUC presence
        let is_muc = xml.contains("xmlns=\"http://jabber.org/protocol/muc#user\"");

        if is_muc {
            // MUC presence
            let room_jid = from.split('/').next().unwrap_or(&from);
            let nick = from.split('/').nth(1).unwrap_or("");

            let mut room = self.mucs.get_or_create(&room_jid.parse::<Jid>().unwrap());

            match ptype.as_str() {
                "unavailable" => {
                    room.remove_occupant(nick);
                }
                _ => {
                    let mut occupant = MucOccupant::new(nick.to_string());
                    // Convert roster PresenceShow to muc PresenceShow
                    let muc_show = match show {
                        PresenceShow::Online => MucPresenceShow::Online,
                        PresenceShow::Away => MucPresenceShow::Away,
                        PresenceShow::Dnd => MucPresenceShow::Dnd,
                        PresenceShow::Xa => MucPresenceShow::Xa,
                        PresenceShow::Unavailable => MucPresenceShow::Unavailable,
                    };
                    occupant.presence.show = muc_show;
                    occupant.presence.status = status;

                    // Parse muc#user x element for role/affiliation
                    if let Some(x_content) =
                        extract_child_content(xml, "x", "http://jabber.org/protocol/muc#user")
                    {
                        if let Some(item_content) = extract_child_content(&x_content, "item", "") {
                            if let Some(role) = extract_attr(&item_content, "role") {
                                occupant.role = match role.as_str() {
                                    "moderator" => MucRole::Moderator,
                                    "owner" => MucRole::Owner,
                                    "admin" => MucRole::Admin,
                                    "visitor" => MucRole::Visitor,
                                    _ => MucRole::Participant,
                                };
                            }
                            if let Some(aff) = extract_attr(&item_content, "affiliation") {
                                occupant.affiliation = match aff.as_str() {
                                    "owner" => MucAffiliation::Owner,
                                    "admin" => MucAffiliation::Admin,
                                    "member" => MucAffiliation::Member,
                                    "outcast" => MucAffiliation::Outcast,
                                    _ => MucAffiliation::None,
                                };
                            }
                        }
                    }

                    room.add_occupant(occupant);
                }
            }
        } else {
            // Regular presence
            if let Ok(from_jid) = from.parse::<Jid>() {
                let presence = Presence {
                    from: from_jid.clone(),
                    show,
                    status,
                    priority: extract_child_text(xml, "priority")
                        .and_then(|p| p.parse().ok())
                        .unwrap_or(0),
                    resource: None,
                    timestamp: chrono::Utc::now(),
                };

                match ptype.as_str() {
                    "subscribe" => {
                        self.roster.subscribe(&from_jid)?;
                    }
                    "subscribed" => {
                        self.roster.approve_subscription(&from_jid)?;
                    }
                    "unsubscribe" => {
                        self.roster.unsubscribe(&from_jid)?;
                    }
                    "unsubscribed" => {
                        self.roster.deny_subscription(&from_jid)?;
                    }
                    "unavailable" => {
                        let mut p = presence.clone();
                        p.show = PresenceShow::Unavailable;
                        let mut item = crate::roster::RosterItem::new(from_jid.clone());
                        item.update_presence(p);
                        self.roster.upsert(item);
                    }
                    "available" | "" => {
                        let mut item = crate::roster::RosterItem::new(from_jid.clone());
                        item.update_presence(presence);
                        self.roster.upsert(item);
                    }
                    "error" => {
                        // Handle error
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    async fn handle_iq(&self, xml: &str) -> Result<()> {
        let _from = extract_attr(xml, "from").unwrap_or_default();
        let _id = extract_attr(xml, "id").unwrap_or_default();
        let iq_type = extract_attr(xml, "type").unwrap_or_else(|| "get".to_string());

        // Handle roster IQ - check for query element
        if xml.contains("xmlns=\"jabber:iq:roster\"") {
            if iq_type == "result" || iq_type == "set" {
                // Extract items from the query
                if let Some(query_content) = extract_child_content(xml, "query", "jabber:iq:roster")
                {
                    for item_xml in extract_children(&query_content, "item") {
                        let jid_str = extract_attr(&item_xml, "jid").unwrap_or_default();
                        if let Ok(jid) = jid_str.parse::<Jid>() {
                            let name = extract_attr(&item_xml, "name");
                            let subscription = extract_attr(&item_xml, "subscription")
                                .map(|s| match s.as_str() {
                                    "both" => SubscriptionState::Both,
                                    "to" => SubscriptionState::To,
                                    "from" => SubscriptionState::From,
                                    "none" => SubscriptionState::None,
                                    "remove" => SubscriptionState::Remove,
                                    _ => SubscriptionState::None,
                                })
                                .unwrap_or(SubscriptionState::None);

                            let groups: Vec<String> = extract_children(&query_content, "group")
                                .into_iter()
                                .map(|g| extract_text(&g).unwrap_or_default())
                                .collect();

                            let mut roster_item = crate::roster::RosterItem::new(jid);
                            roster_item.name = name;
                            roster_item.subscription = subscription;
                            roster_item.groups = groups;
                            self.roster.upsert(roster_item);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Simple XML helper functions
fn extract_element_name(xml: &str) -> Result<String> {
    // Find the first element name
    if let Some(start) = xml.find('<') {
        let after_lt = &xml[start + 1..];
        if let Some(end) = after_lt.find([' ', '>', '/']) {
            return Ok(after_lt[..end].to_string());
        }
    }
    Err(XmppError::ParseError(
        "Could not find element name".to_string(),
    ))
}

fn extract_attr(xml: &str, name: &str) -> Option<String> {
    let pattern = format!("{}=\"", name);
    if let Some(start) = xml.find(&pattern) {
        let after_eq = &xml[start + pattern.len()..];
        if let Some(end) = after_eq.find('"') {
            return Some(after_eq[..end].to_string());
        }
    }
    None
}

fn extract_child_text(xml: &str, child_name: &str) -> Option<String> {
    let start_tag = format!("<{}>", child_name);
    let end_tag = format!("</{}>", child_name);

    if let Some(start) = xml.find(&start_tag) {
        let after_start = &xml[start + start_tag.len()..];
        if let Some(end) = after_start.find(&end_tag) {
            return Some(after_start[..end].to_string());
        }
    }
    None
}

fn extract_child_content(xml: &str, child_name: &str, xmlns: &str) -> Option<String> {
    let start_tag = if xmlns.is_empty() {
        format!("<{}>", child_name)
    } else {
        format!("<{} xmlns=\"{}\">", child_name, xmlns)
    };
    let end_tag = format!("</{}>", child_name);

    if let Some(start) = xml.find(&start_tag) {
        let after_start = &xml[start + start_tag.len()..];
        if let Some(end) = after_start.find(&end_tag) {
            return Some(after_start[..end].to_string());
        }
    }
    None
}

fn extract_children(xml: &str, child_name: &str) -> Vec<String> {
    let mut results = Vec::new();
    let start_tag = format!("<{}>", child_name);
    let end_tag = format!("</{}>", child_name);

    let mut search_start = 0;
    while let Some(start) = xml[search_start..].find(&start_tag) {
        let absolute_start = search_start + start;
        let after_start = &xml[absolute_start + start_tag.len()..];
        if let Some(end) = after_start.find(&end_tag) {
            let content = after_start[..end].to_string();
            results.push(content);
            search_start = absolute_start + start_tag.len() + end + end_tag.len();
        } else {
            break;
        }
    }
    results
}

fn extract_text(xml: &str) -> Option<String> {
    // Simple text extraction - get text between tags
    if let Some(start) = xml.find('>') {
        let after_gt = &xml[start + 1..];
        if let Some(end) = after_gt.find('<') {
            return Some(after_gt[..end].to_string());
        }
    }
    None
}
