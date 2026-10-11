//! XEP-0050 ad-hoc commands (also the transport for XEP-0100 gateways).

use minidom::Element;

pub const NS_COMMANDS: &str = "http://jabber.org/protocol/commands";
pub const NS_DISCO_ITEMS: &str = "http://jabber.org/protocol/disco#items";

/// A command advertised by an entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdHocCommand {
    pub jid: String,
    pub node: String,
    pub name: Option<String>,
}

/// The reply to an executed command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdHocResponse {
    pub status: String,
    pub sessionid: Option<String>,
    pub form: Option<String>,
    pub notes: Vec<String>,
}

/// Parse the `disco#items` result for the commands node.
#[must_use]
pub fn parse_command_list(jid: &str, items: &Element) -> Vec<AdHocCommand> {
    items
        .children()
        .filter(|child| child.name() == "item")
        .filter_map(|item| {
            let node = item.attr("node")?;
            if !node.starts_with("http://jabber.org/protocol/commands#") && node.is_empty() {
                return None;
            }
            Some(AdHocCommand {
                jid: jid.to_string(),
                node: node.to_string(),
                name: item.attr("name").map(str::to_string),
            })
        })
        .collect()
}

/// Parse a `<command/>` payload.
#[must_use]
pub fn parse_response(command: &Element) -> AdHocResponse {
    let status = command.attr("status").unwrap_or("executing").to_string();
    let form = command
        .get_child("x", "jabber:x:data")
        .map(crate::vcard::to_xml);
    let notes = command
        .children()
        .filter(|child| child.name() == "note")
        .map(minidom::Element::text)
        .filter(|note| !note.trim().is_empty())
        .collect();
    AdHocResponse {
        status,
        sessionid: command.attr("sessionid").map(str::to_string),
        form,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn lists_commands() {
        let items = Element::from_str(
            r#"<query xmlns="http://jabber.org/protocol/disco#items" node="http://jabber.org/protocol/commands"><item jid="example.org" node="http://jabber.org/protocol/admin#get-online-users" name="Get Online Users"/></query>"#,
        )
        .unwrap();
        let commands = parse_command_list("example.org", &items);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name.as_deref(), Some("Get Online Users"));
    }

    #[test]
    fn parses_a_command_reply() {
        let command = Element::from_str(
            r#"<command xmlns="http://jabber.org/protocol/commands" node="n" status="completed" sessionid="s1"><note type="info">done</note></command>"#,
        )
        .unwrap();
        let response = parse_response(&command);
        assert_eq!(response.status, "completed");
        assert_eq!(response.sessionid.as_deref(), Some("s1"));
        assert_eq!(response.notes, vec!["done".to_string()]);
    }
}
