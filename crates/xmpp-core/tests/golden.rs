//! Golden XMPP stanzas for every XEP the client currently handles.

use std::str::FromStr;

use xmpp_parsers::minidom::Element;

const GOLDEN: &[(&str, &str, &str)] = &[
    (
        include_str!("golden/message_chat.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/carbon_sent.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/carbon_received.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/mam_result.xml"),
        "message",
        "jabber:client",
    ),
    (include_str!("golden/mam_fin.xml"), "iq", "jabber:client"),
    (
        include_str!("golden/receipt_request.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/receipt_received.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/marker_displayed.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/reaction.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/correction.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/retraction.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/muc_message.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/presence_subscribe.xml"),
        "presence",
        "jabber:client",
    ),
    (
        include_str!("golden/presence_subscribed.xml"),
        "presence",
        "jabber:client",
    ),
    (
        include_str!("golden/roster_push.xml"),
        "iq",
        "jabber:client",
    ),
    (
        include_str!("golden/blocklist_result.xml"),
        "iq",
        "jabber:client",
    ),
    (include_str!("golden/block_push.xml"), "iq", "jabber:client"),
    (include_str!("golden/disco_info.xml"), "iq", "jabber:client"),
    (
        include_str!("golden/extdisco_result.xml"),
        "iq",
        "jabber:client",
    ),
    (
        include_str!("golden/sm_enabled.xml"),
        "enabled",
        "urn:xmpp:sm:3",
    ),
    (
        include_str!("golden/sm_resumed.xml"),
        "resumed",
        "urn:xmpp:sm:3",
    ),
    (include_str!("golden/bookmarks.xml"), "iq", "jabber:client"),
    (
        include_str!("golden/pep_tune_event.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/pep_vcard4_event.xml"),
        "message",
        "jabber:client",
    ),
    (
        include_str!("golden/server_info.xml"),
        "iq",
        "jabber:client",
    ),
    (
        include_str!("golden/presence_caps.xml"),
        "presence",
        "jabber:client",
    ),
];

#[test]
fn every_golden_stanza_parses() {
    for (xml, name, ns) in GOLDEN {
        let element = Element::from_str(xml).unwrap_or_else(|error| {
            panic!("golden stanza failed to parse: {error}\n{xml}");
        });
        assert_eq!(element.name(), *name, "unexpected root name in:\n{xml}");
        assert_eq!(element.ns(), *ns, "unexpected root namespace in:\n{xml}");
    }
}

#[test]
fn golden_files_are_not_empty() {
    for (xml, _, _) in GOLDEN {
        assert!(xml.contains('<'), "golden fixture is empty");
    }
}
