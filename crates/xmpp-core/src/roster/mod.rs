//! Roster (contact list) management

use dioxus::prelude::{Readable, Signal, Writable};
use jid::Jid;
use minidom::Element;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

use crate::{EventBus, Result, XmppEvent};
use xmpp_parsers::ns;

/// Convert minidom Element to String for serialization
fn element_to_string(element: &Element) -> String {
    let mut buf = Vec::new();
    element.write_to(&mut buf).unwrap_or_default();
    String::from_utf8(buf).unwrap_or_default()
}

/// Roster manager for contact list
pub struct Roster {
    items: RwLock<HashMap<String, RosterItem>>,
    groups: RwLock<HashMap<String, RosterGroup>>,
    events: Arc<EventBus>,
    changed: Arc<RwLock<Signal<u64>>>,
}

impl Roster {
    pub fn new(events: Arc<EventBus>) -> Self {
        Self {
            items: RwLock::new(HashMap::new()),
            groups: RwLock::new(HashMap::new()),
            events,
            changed: Arc::new(RwLock::new(Signal::new(0))),
        }
    }

    pub fn get(&self, jid: &Jid) -> Option<RosterItem> {
        let bare_jid = jid.to_bare().to_string();
        self.items.read().get(&bare_jid).cloned()
    }

    pub fn get_by_bare(&self, bare: &str) -> Option<RosterItem> {
        self.items.read().get(bare).cloned()
    }

    pub fn get_mut(&self, jid: &Jid) -> Option<RosterItem> {
        let bare_jid = jid.to_bare().to_string();
        self.items.write().get(&bare_jid).cloned()
    }

    pub fn upsert(&self, item: RosterItem) {
        let bare_jid = item.jid.to_bare().to_string();
        self.items.write().insert(bare_jid, item);
        let mut changed = self.changed.write();
        let changed_val = *changed.read();
        changed.set(changed_val + 1);
        self.emit_roster_changed();
    }

    pub fn remove(&self, jid: &Jid) -> Option<RosterItem> {
        let bare_jid = jid.to_bare().to_string();
        let removed = self.items.write().remove(&bare_jid);
        if removed.is_some() {
            let mut changed = self.changed.write();
            let changed_val = *changed.read();
            changed.set(changed_val + 1);
            self.emit_roster_changed();
        }
        removed
    }

    pub fn list(&self) -> Vec<RosterItem> {
        self.items.read().values().cloned().collect()
    }

    pub fn list_by_group(&self, group: &str) -> Vec<RosterItem> {
        self.items
            .read()
            .values()
            .filter(|item| item.groups.contains(&group.to_string()))
            .cloned()
            .collect()
    }

    pub fn groups(&self) -> Vec<RosterGroup> {
        self.groups.read().values().cloned().collect()
    }

    pub fn add_group(&self, group: RosterGroup) {
        self.groups.write().insert(group.name.clone(), group);
        let mut changed = self.changed.write();
        let changed_val = *changed.read();
        changed.set(changed_val + 1);
        self.emit_roster_changed();
    }

    pub fn remove_group(&self, name: &str) -> Option<RosterGroup> {
        let removed = self.groups.write().remove(name);
        if removed.is_some() {
            let mut changed = self.changed.write();
            let changed_val = *changed.read();
            changed.set(changed_val + 1);
            self.emit_roster_changed();
        }
        removed
    }

    pub fn subscribe(&self, _jid: &Jid) -> Result<()> {
        // Send subscription request - emit event for connection to handle
        let stanza = minidom::Element::builder("presence", ns::JABBER_CLIENT)
            .attr("type", "subscribe")
            .attr("to", _jid.to_string())
            .build();
        self.emit_event(XmppEvent::SendStanza(element_to_string(&stanza)));
        Ok(())
    }

    pub fn unsubscribe(&self, _jid: &Jid) -> Result<()> {
        let stanza = minidom::Element::builder("presence", ns::JABBER_CLIENT)
            .attr("type", "unsubscribe")
            .attr("to", _jid.to_string())
            .build();
        self.emit_event(XmppEvent::SendStanza(element_to_string(&stanza)));
        Ok(())
    }

    pub fn approve_subscription(&self, _jid: &Jid) -> Result<()> {
        let stanza = minidom::Element::builder("presence", ns::JABBER_CLIENT)
            .attr("type", "subscribed")
            .attr("to", _jid.to_string())
            .build();
        self.emit_event(XmppEvent::SendStanza(element_to_string(&stanza)));
        Ok(())
    }

    pub fn deny_subscription(&self, _jid: &Jid) -> Result<()> {
        let stanza = minidom::Element::builder("presence", ns::JABBER_CLIENT)
            .attr("type", "unsubscribed")
            .attr("to", _jid.to_string())
            .build();
        self.emit_event(XmppEvent::SendStanza(element_to_string(&stanza)));
        Ok(())
    }

    pub fn block(&self, jid: &Jid) -> Result<()> {
        // XEP-0191: Blocking Command
        let stanza = format!(
            "<iq type='set' id='block-{}' xmlns='{}'><block xmlns='urn:xmpp:blocking'><item jid='{}'/></block></iq>",
            jid,
            ns::JABBER_CLIENT,
            jid
        );
        self.emit_event(XmppEvent::SendStanza(stanza));

        // Mark as blocked locally
        let bare_jid = jid.to_bare().to_string();
        if let Some(item) = self.items.write().get_mut(&bare_jid) {
            item.is_blocked = true;
        }

        Ok(())
    }

    pub fn unblock(&self, jid: &Jid) -> Result<()> {
        // XEP-0191: Blocking Command
        let stanza = format!(
            "<iq type='set' id='unblock-{}' xmlns='{}'><unblock xmlns='urn:xmpp:blocking'><item jid='{}'/></unblock></iq>",
            jid,
            ns::JABBER_CLIENT,
            jid
        );
        self.emit_event(XmppEvent::SendStanza(stanza));

        // Mark as unblocked locally
        let bare_jid = jid.to_bare().to_string();
        if let Some(item) = self.items.write().get_mut(&bare_jid) {
            item.is_blocked = false;
        }

        Ok(())
    }

    pub fn changed_signal(&self) -> Signal<u64> {
        self.changed.read().clone()
    }

    fn emit_roster_changed(&self) {
        let _ = self.events.send(XmppEvent::RosterChanged);
    }

    fn emit_event(&self, event: XmppEvent) {
        let _ = self.events.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventBus;

    #[test]
    #[ignore = "Requires Dioxus context for Signal"]
    fn test_roster_new() {
        let events = Arc::new(EventBus::new());
        let roster = Roster::new(events);
        assert_eq!(roster.list().len(), 0);
    }

    #[test]
    #[ignore = "Requires Dioxus context for Signal"]
    fn test_roster_upsert_and_get() {
        let events = Arc::new(EventBus::new());
        let roster = Roster::new(events);

        let jid = "user@example.com".parse::<Jid>().unwrap();
        let item = RosterItem::new(jid.clone());

        roster.upsert(item.clone());
        let retrieved = roster.get(&jid);

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().jid, jid);
    }

    #[test]
    #[ignore = "Requires Dioxus context for Signal"]
    fn test_roster_remove() {
        let events = Arc::new(EventBus::new());
        let roster = Roster::new(events);

        let jid = "user@example.com".parse::<Jid>().unwrap();
        let item = RosterItem::new(jid.clone());

        roster.upsert(item);
        assert!(roster.get(&jid).is_some());

        roster.remove(&jid);
        assert!(roster.get(&jid).is_none());
    }

    #[test]
    #[ignore = "Requires Dioxus context for Signal"]
    fn test_roster_groups() {
        let events = Arc::new(EventBus::new());
        let roster = Roster::new(events);

        let group = RosterGroup::new("Friends".to_string());
        roster.add_group(group);

        let groups = roster.groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Friends");
    }
}

/// Roster item (contact)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RosterItem {
    pub jid: Jid,
    pub name: Option<String>,
    pub groups: Vec<String>,
    pub subscription: SubscriptionState,
    pub ask: Option<SubscriptionState>,
    pub presence: HashMap<String, Presence>,
    pub avatar_hash: Option<String>,
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub avatar_data: Option<String>,
    #[serde(default)]
    pub avatar_mime: Option<String>,
    pub vcard: Option<serde_json::Value>,
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
    pub is_blocked: bool,
    pub is_pending_out: bool,
    pub is_pending_in: bool,
}

impl RosterItem {
    pub fn new(jid: Jid) -> Self {
        Self {
            jid,
            name: None,
            groups: Vec::new(),
            subscription: SubscriptionState::None,
            ask: None,
            presence: HashMap::new(),
            avatar_hash: None,
            avatar_url: None,
            avatar_data: None,
            avatar_mime: None,
            vcard: None,
            last_seen: None,
            is_blocked: false,
            is_pending_out: false,
            is_pending_in: false,
        }
    }

    pub fn update_presence(&mut self, presence: Presence) {
        let resource = presence
            .resource
            .clone()
            .unwrap_or_else(|| "default".to_string());
        self.presence.insert(resource, presence);

        // Update last seen for offline
        if self
            .presence
            .values()
            .any(|p| p.show == PresenceShow::Unavailable)
        {
            self.last_seen = Some(chrono::Utc::now());
        }
    }

    /// True if any connected resource is not unavailable.
    pub fn is_online(&self) -> bool {
        self.presence
            .values()
            .any(|p| p.show != PresenceShow::Unavailable)
    }

    /// The best (most available) presence show across resources.
    pub fn best_show(&self) -> PresenceShow {
        if self.is_online() {
            self.presence
                .values()
                .map(|p| p.show)
                .find(|s| *s != PresenceShow::Unavailable)
                .unwrap_or(PresenceShow::Online)
        } else {
            PresenceShow::Unavailable
        }
    }

    /// Display name: the roster name, or the JID node/localpart.
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| self.jid.to_bare().to_string())
    }
}

/// Roster group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterGroup {
    pub name: String,
    pub contacts: Vec<String>, // Bare JIDs
}

impl RosterGroup {
    pub fn new(name: String) -> Self {
        Self {
            name,
            contacts: Vec::new(),
        }
    }
}

/// Subscription state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionState {
    None,
    To,
    From,
    Both,
    Remove,
}

impl Default for SubscriptionState {
    fn default() -> Self {
        Self::None
    }
}

/// Presence information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    pub from: Jid,
    pub show: PresenceShow,
    pub status: Option<String>,
    pub priority: i32,
    pub resource: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Presence {
    pub fn new(from: Jid) -> Self {
        Self {
            from,
            show: PresenceShow::Online,
            status: None,
            priority: 0,
            resource: None,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Presence show values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresenceShow {
    Online,
    Away,
    Dnd,
    Xa,
    Unavailable,
}

impl Default for PresenceShow {
    fn default() -> Self {
        Self::Online
    }
}
