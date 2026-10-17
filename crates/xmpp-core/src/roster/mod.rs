//! Roster (contact list) management

use jid::Jid;
use minidom::Element;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::change::ChangeCounter;
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
    changed: Arc<ChangeCounter>,
}

impl Roster {
    pub fn new(events: Arc<EventBus>) -> Self {
        Self {
            items: RwLock::new(HashMap::new()),
            groups: RwLock::new(HashMap::new()),
            events,
            changed: Arc::new(ChangeCounter::default()),
        }
    }

    /// Drop every contact and group, e.g. when switching accounts.
    pub fn clear(&self) {
        self.items.write().clear();
        self.groups.write().clear();
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
        self.changed.bump();
        self.emit_roster_changed();
    }

    pub fn remove(&self, jid: &Jid) -> Option<RosterItem> {
        let bare_jid = jid.to_bare().to_string();
        let removed = self.items.write().remove(&bare_jid);
        if removed.is_some() {
            self.changed.bump();
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
        self.changed.bump();
        self.emit_roster_changed();
    }

    pub fn remove_group(&self, name: &str) -> Option<RosterGroup> {
        let removed = self.groups.write().remove(name);
        if removed.is_some() {
            self.changed.bump();
            self.emit_roster_changed();
        }
        removed
    }

    pub fn subscribe(&self, jid: &Jid) -> Result<()> {
        let stanza = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("type", "subscribe")
            .attr("to", jid.to_string())
            .build();
        self.emit_event(XmppEvent::SendStanza(element_to_string(&stanza)));

        let mut items = self.items.write();
        let item = items
            .entry(jid.to_bare().to_string())
            .or_insert_with(|| RosterItem::new(jid.clone()));
        item.is_pending_out = true;
        item.ask = Some(SubscriptionState::Subscribe);
        drop(items);
        self.changed.bump();
        self.emit_roster_changed();
        Ok(())
    }

    pub fn unsubscribe(&self, jid: &Jid) -> Result<()> {
        let stanza = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("type", "unsubscribe")
            .attr("to", jid.to_string())
            .build();
        self.emit_event(XmppEvent::SendStanza(element_to_string(&stanza)));

        let mut items = self.items.write();
        if let Some(item) = items.get_mut(&jid.to_bare().to_string()) {
            item.subscription = match item.subscription {
                SubscriptionState::Both | SubscriptionState::To => SubscriptionState::To,
                _ => SubscriptionState::None,
            };
            item.is_pending_out = false;
            item.ask = None;
        }
        drop(items);
        self.changed.bump();
        self.emit_roster_changed();
        Ok(())
    }

    /// Surface an incoming subscription request to the presentation layer.
    pub fn notify_subscription_request(&self, from: &Jid) {
        self.emit_event(XmppEvent::SubscriptionRequest {
            from: from.to_string(),
        });
        self.emit_roster_changed();
    }

    pub fn approve_subscription(&self, jid: &Jid) -> Result<()> {
        let stanza = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("type", "subscribed")
            .attr("to", jid.to_string())
            .build();
        self.emit_event(XmppEvent::SendStanza(element_to_string(&stanza)));

        let mut items = self.items.write();
        if let Some(item) = items.get_mut(&jid.to_bare().to_string()) {
            item.is_pending_in = false;
            item.subscription = match item.subscription {
                SubscriptionState::To | SubscriptionState::Both => SubscriptionState::Both,
                _ => SubscriptionState::From,
            };
        }
        drop(items);
        self.changed.bump();
        self.emit_roster_changed();
        Ok(())
    }

    pub fn deny_subscription(&self, jid: &Jid) -> Result<()> {
        let stanza = Element::builder("presence", ns::JABBER_CLIENT)
            .attr("type", "unsubscribed")
            .attr("to", jid.to_string())
            .build();
        self.emit_event(XmppEvent::SendStanza(element_to_string(&stanza)));

        let mut items = self.items.write();
        if let Some(item) = items.get_mut(&jid.to_bare().to_string()) {
            item.is_pending_in = false;
        }
        drop(items);
        self.changed.bump();
        self.emit_roster_changed();
        Ok(())
    }

    pub fn block(&self, jid: &Jid) -> Result<()> {
        let stanza =
            Element::builder("iq", ns::JABBER_CLIENT)
                .attr("type", "set")
                .attr("id", format!("block-{jid}"))
                .append(Element::builder("block", "urn:xmpp:blocking").append(
                    Element::builder("item", "urn:xmpp:blocking").attr("jid", jid.to_string()),
                ))
                .build();
        self.emit_event(XmppEvent::SendStanza(element_to_string(&stanza)));

        let mut items = self.items.write();
        if let Some(item) = items.get_mut(&jid.to_bare().to_string()) {
            item.is_blocked = true;
            item.presence.clear();
        }
        drop(items);
        self.changed.bump();
        self.emit_roster_changed();
        Ok(())
    }

    pub fn unblock(&self, jid: &Jid) -> Result<()> {
        let stanza =
            Element::builder("iq", ns::JABBER_CLIENT)
                .attr("type", "set")
                .attr("id", format!("unblock-{jid}"))
                .append(Element::builder("unblock", "urn:xmpp:blocking").append(
                    Element::builder("item", "urn:xmpp:blocking").attr("jid", jid.to_string()),
                ))
                .build();
        self.emit_event(XmppEvent::SendStanza(element_to_string(&stanza)));

        let mut items = self.items.write();
        if let Some(item) = items.get_mut(&jid.to_bare().to_string()) {
            item.is_blocked = false;
        }
        drop(items);
        self.changed.bump();
        self.emit_roster_changed();
        Ok(())
    }

    /// Apply a server block/unblock push for one JID.
    pub fn set_blocked(&self, jid: &Jid, blocked: bool) {
        let mut items = self.items.write();
        let item = items
            .entry(jid.to_bare().to_string())
            .or_insert_with(|| RosterItem::new(jid.clone()));
        item.is_blocked = blocked;
        if blocked {
            item.presence.clear();
        }
        drop(items);
        self.changed.bump();
        self.emit_roster_changed();
    }

    /// Ask the server for the XEP-0191 blocklist.
    pub fn request_blocklist(&self) {
        let stanza = Element::builder("iq", ns::JABBER_CLIENT)
            .attr("type", "get")
            .attr("id", "blocklist-init")
            .append(Element::builder("blocklist", "urn:xmpp:blocking"))
            .build();
        self.emit_event(XmppEvent::SendStanza(element_to_string(&stanza)));
    }

    /// Apply the authoritative blocklist: everything not listed is unblocked.
    pub fn set_blocklist(&self, blocked: &[Jid]) {
        let blocked: Vec<String> = blocked
            .iter()
            .map(|jid| jid.to_bare().to_string())
            .collect();
        let mut items = self.items.write();
        for (bare, item) in items.iter_mut() {
            item.is_blocked = blocked.iter().any(|jid| jid == bare);
        }
        drop(items);
        self.changed.bump();
        self.emit_roster_changed();
    }

    /// Surface an approved subscription to the presentation layer.
    pub fn notify_subscription_approved(&self, jid: &Jid) {
        self.emit_event(XmppEvent::SubscriptionApproved {
            jid: jid.to_string(),
        });
    }

    /// Surface a denied/revoked subscription to the presentation layer.
    pub fn notify_subscription_denied(&self, jid: &Jid) {
        self.emit_event(XmppEvent::SubscriptionDenied {
            jid: jid.to_string(),
        });
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
    fn test_roster_new() {
        let events = Arc::new(EventBus::new());
        let roster = Roster::new(events);
        assert_eq!(roster.list().len(), 0);
    }

    #[test]
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
    fn vcard4_fields_take_precedence_over_vcard_temp() {
        let jid = "user@example.com".parse::<Jid>().unwrap();
        let mut item = RosterItem::new(jid);
        item.vcard = Some(
            r#"<vCard xmlns="vcard-temp"><FN>Old Name</FN><ORG>Old Org</ORG><TITLE>Old Title</TITLE></vCard>"#
                .into(),
        );
        item.vcard4 = Some(
            r#"<vcard xmlns="urn:xmpp:vcard4"><fn>New Name</fn><title>New Title</title></vcard>"#
                .into(),
        );

        assert_eq!(item.vcard_field("FN").as_deref(), Some("New Name"));
        assert_eq!(item.vcard_field("TITLE").as_deref(), Some("New Title"));
        assert_eq!(item.vcard_field("ORG").as_deref(), Some("Old Org"));
        assert_eq!(item.vcard4_field("FN").as_deref(), Some("New Name"));
        assert_eq!(item.vcard4_field("ORG"), None);
    }

    #[test]
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
    fn test_roster_groups() {
        let events = Arc::new(EventBus::new());
        let roster = Roster::new(events);

        let group = RosterGroup::new("Friends".to_string());
        roster.add_group(group);

        let groups = roster.groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Friends");
    }

    #[test]
    fn test_subscribe_marks_pending_out() {
        let events = Arc::new(EventBus::new());
        let roster = Roster::new(events);
        let jid = "user@example.com".parse::<Jid>().unwrap();

        roster.subscribe(&jid).unwrap();
        let item = roster.get(&jid).unwrap();
        assert!(item.is_pending_out);
        assert_eq!(item.ask, Some(SubscriptionState::Subscribe));

        roster.unsubscribe(&jid).unwrap();
        let item = roster.get(&jid).unwrap();
        assert!(!item.is_pending_out);
        assert_eq!(item.ask, None);
    }

    #[test]
    fn test_blocklist_is_authoritative() {
        let events = Arc::new(EventBus::new());
        let roster = Roster::new(events);
        let bob = "bob@example.com".parse::<Jid>().unwrap();
        let carol = "carol@example.com".parse::<Jid>().unwrap();
        roster.upsert(RosterItem::new(bob.clone()));
        roster.upsert(RosterItem::new(carol.clone()));

        roster.set_blocked(&bob, true);
        assert!(roster.get(&bob).unwrap().is_blocked);
        assert!(!roster.get(&carol).unwrap().is_blocked);

        roster.set_blocklist(&[carol.clone()]);
        assert!(!roster.get(&bob).unwrap().is_blocked);
        assert!(roster.get(&carol).unwrap().is_blocked);
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
    #[serde(default)]
    pub vcard: Option<String>,
    #[serde(default)]
    pub vcard4: Option<String>,
    #[serde(default)]
    pub tune: Option<String>,
    #[serde(default)]
    pub mood: Option<String>,
    #[serde(default)]
    pub activity: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub caps: Option<CapsInfo>,
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
            vcard4: None,
            tune: None,
            mood: None,
            activity: None,
            location: None,
            caps: None,
            last_seen: None,
            is_blocked: false,
            is_pending_out: false,
            is_pending_in: false,
        }
    }

    /// Read a text field from one of the stored PEP payloads.
    #[must_use]
    pub fn pep_field(&self, xml: Option<&str>, name: &str) -> Option<String> {
        let xml = xml?;
        let payload = xml.parse::<Element>().ok()?;
        crate::vcard::field_text(&payload, name)
    }

    /// A short "artist - title" summary of the stored tune.
    #[must_use]
    pub fn tune_summary(&self) -> Option<String> {
        let artist = self.pep_field(self.tune.as_deref(), "artist");
        let title = self.pep_field(self.tune.as_deref(), "title");
        match (artist, title) {
            (Some(artist), Some(title)) => Some(format!("{artist} - {title}")),
            (Some(artist), None) => Some(artist),
            (None, Some(title)) => Some(title),
            (None, None) => None,
        }
    }

    /// The mood value, if one is published.
    #[must_use]
    pub fn mood_value(&self) -> Option<String> {
        let xml = self.mood.as_deref()?;
        let payload = xml.parse::<Element>().ok()?;
        payload
            .children()
            .find(|child| child.name() != "text")
            .map(|child| child.name().to_string())
    }

    /// The activity value, if one is published.
    #[must_use]
    pub fn activity_value(&self) -> Option<String> {
        let xml = self.activity.as_deref()?;
        let payload = xml.parse::<Element>().ok()?;
        payload
            .children()
            .find(|child| child.name() != "text")
            .map(|child| child.name().to_string())
    }

    /// Read a field from vCard4 first, then from vCard-temp.
    #[must_use]
    pub fn vcard_field(&self, name: &str) -> Option<String> {
        if let Some(field) = self.vcard4_field(name) {
            return Some(field);
        }
        let xml = self.vcard.as_deref()?;
        let vcard = xml.parse::<Element>().ok()?;
        crate::vcard::field_text(&vcard, name)
    }

    /// Read a field from the stored vCard4 XML only.
    #[must_use]
    pub fn vcard4_field(&self, name: &str) -> Option<String> {
        crate::vcard4::field_text(self.vcard4.as_deref()?, name)
    }

    /// The parsed vCard4 fields, when a payload was received.
    #[must_use]
    pub fn vcard4(&self) -> Option<crate::vcard4::VCard4> {
        crate::vcard4::VCard4::parse(self.vcard4.as_deref()?)
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

/// XEP-0115 capability advertisement from a presence.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CapsInfo {
    pub node: String,
    pub ver: String,
    #[serde(default)]
    pub hash: Option<String>,
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
    Subscribe,
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
