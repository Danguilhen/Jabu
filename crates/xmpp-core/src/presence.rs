//! Presence stanza construction shared by the client and the processor.

use chrono::{DateTime, Utc};
use minidom::Element;
use xmpp_parsers::ns;

/// XEP-0319 Last User Interaction in Presence.
pub const NS_IDLE: &str = "urn:xmpp:idle:1";

/// Build an available presence with the optional show/status/priority/nick.
#[must_use]
pub fn presence_element(
    show: Option<&str>,
    status: Option<&str>,
    priority: i32,
    nick: Option<&str>,
) -> Element {
    presence_element_with_idle(show, status, priority, nick, None)
}

/// Build a presence with an optional XEP-0319 idle timestamp.
#[must_use]
pub fn presence_element_with_idle(
    show: Option<&str>,
    status: Option<&str>,
    priority: i32,
    nick: Option<&str>,
    idle_since: Option<DateTime<Utc>>,
) -> Element {
    let mut presence = Element::builder("presence", ns::JABBER_CLIENT);
    if let Some(show) = show.filter(|show| !show.is_empty()) {
        presence = presence.append(Element::builder("show", ns::JABBER_CLIENT).append(show));
    }
    if let Some(status) = status.filter(|status| !status.is_empty()) {
        presence = presence.append(Element::builder("status", ns::JABBER_CLIENT).append(status));
    }
    presence = presence
        .append(Element::builder("priority", ns::JABBER_CLIENT).append(priority.to_string()))
        .append(crate::caps::caps_element());
    if let Some(nick) = nick.filter(|nick| !nick.is_empty()) {
        presence = presence
            .append(Element::builder("nick", "http://jabber.org/protocol/nick").append(nick));
    }
    if let Some(idle_since) = idle_since {
        presence = presence
            .append(Element::builder("idle", NS_IDLE).attr("since", idle_since.to_rfc3339()));
    }
    presence.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_show_status_priority_and_nick() {
        let presence = presence_element(Some("away"), Some("lunch"), 5, Some("ali"));
        assert_eq!(presence.name(), "presence");
        assert_eq!(
            presence
                .get_child("show", ns::JABBER_CLIENT)
                .unwrap()
                .text(),
            "away"
        );
        assert_eq!(
            presence
                .get_child("status", ns::JABBER_CLIENT)
                .unwrap()
                .text(),
            "lunch"
        );
        assert_eq!(
            presence
                .get_child("priority", ns::JABBER_CLIENT)
                .unwrap()
                .text(),
            "5"
        );
        assert_eq!(
            presence
                .get_child("nick", "http://jabber.org/protocol/nick")
                .unwrap()
                .text(),
            "ali"
        );
    }

    #[test]
    fn online_presence_is_bare() {
        let presence = presence_element(None, None, 0, None);
        assert!(presence.get_child("show", ns::JABBER_CLIENT).is_none());
        assert!(presence.get_child("status", ns::JABBER_CLIENT).is_none());
        assert!(presence.get_child("nick", minidom::NSChoice::Any).is_none());
        assert!(presence.get_child("idle", NS_IDLE).is_none());
    }

    #[test]
    fn idle_presence_carries_the_since_timestamp() {
        let since: DateTime<Utc> = "2026-11-07T09:30:00Z".parse().expect("timestamp");
        let presence = presence_element_with_idle(None, None, 0, None, Some(since));
        let idle = presence.get_child("idle", NS_IDLE).expect("idle");
        assert_eq!(idle.attr("since"), Some("2026-11-07T09:30:00+00:00"));
    }
}
