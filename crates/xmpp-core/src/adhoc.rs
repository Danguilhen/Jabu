//! XEP-0050 ad-hoc commands (also the transport for XEP-0100 gateways).

use jid::Jid;
use minidom::Element;

pub const NS_COMMANDS: &str = "http://jabber.org/protocol/commands";
pub const NS_DISCO_ITEMS: &str = "http://jabber.org/protocol/disco#items";
pub const NS_DATA_FORMS: &str = "jabber:x:data";

/// A command advertised by an entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdHocCommand {
    pub jid: String,
    pub node: String,
    pub name: Option<String>,
}

/// Execution state returned by the responder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdHocStatus {
    #[default]
    Executing,
    Completed,
    Canceled,
}

impl AdHocStatus {
    #[must_use]
    pub fn from_attr(value: &str) -> Self {
        match value {
            "completed" => Self::Completed,
            "canceled" => Self::Canceled,
            _ => Self::Executing,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Executing => "executing",
            Self::Completed => "completed",
            Self::Canceled => "canceled",
        }
    }
}

/// The actions a multi-stage command allows at this stage.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdHocActions {
    /// The `execute` attribute, which names the default action.
    pub execute: Option<String>,
    pub prev: bool,
    pub next: bool,
    pub complete: bool,
}

impl AdHocActions {
    /// The action used by a plain "execute"/Enter request.
    ///
    /// XEP-0050: with an `<actions/>` element the default is the `execute`
    /// attribute, or `next` when the attribute is absent.
    #[must_use]
    pub fn default_action(&self) -> &'static str {
        match self.execute.as_deref() {
            Some("prev") => "prev",
            Some("complete") => "complete",
            Some("cancel") => "cancel",
            _ => "next",
        }
    }
}

/// One `<note/>` shown with a command result.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdHocNote {
    /// `info`, `warn` or `error`.
    pub kind: String,
    pub text: String,
}

/// The reply to an executed command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdHocResponse {
    pub status: AdHocStatus,
    pub sessionid: Option<String>,
    pub form: Option<crate::data_form::DataForm>,
    pub notes: Vec<AdHocNote>,
    /// Present for multi-stage commands; `None` means a single-stage result.
    pub actions: Option<AdHocActions>,
    /// Serialized non-form payload, when the responder sends one.
    pub payload: Option<String>,
}

impl AdHocResponse {
    /// The action a plain execute/Enter request should use.
    #[must_use]
    pub fn default_action(&self) -> &'static str {
        self.actions
            .as_ref()
            .map_or("complete", AdHocActions::default_action)
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.status != AdHocStatus::Executing
    }
}

/// Parse the `disco#items` result for the commands node.
#[must_use]
pub fn parse_command_list(jid: &str, items: &Element) -> Vec<AdHocCommand> {
    items
        .children()
        .filter(|child| child.name() == "item")
        .filter_map(|item| {
            let node = item.attr("node")?;
            if node.is_empty() {
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
    let status = AdHocStatus::from_attr(command.attr("status").unwrap_or("executing"));
    let sessionid = command.attr("sessionid").map(str::to_string);
    let form = command
        .get_child("x", NS_DATA_FORMS)
        .map(crate::data_form::parse);
    let notes = command
        .children()
        .filter(|child| child.name() == "note")
        .map(|note| AdHocNote {
            kind: note.attr("type").unwrap_or("info").to_string(),
            text: note.text(),
        })
        .filter(|note| !note.text.trim().is_empty())
        .collect();
    let actions = command
        .get_child("actions", NS_COMMANDS)
        .map(|actions| AdHocActions {
            execute: actions.attr("execute").map(str::to_string),
            prev: actions.get_child("prev", NS_COMMANDS).is_some(),
            next: actions.get_child("next", NS_COMMANDS).is_some(),
            complete: actions.get_child("complete", NS_COMMANDS).is_some(),
        });
    let payload = command
        .children()
        .find(|child| child.name() == "payload")
        .map(crate::vcard::to_xml);

    AdHocResponse {
        status,
        sessionid,
        form,
        notes,
        actions,
        payload,
    }
}

/// Build the IQ for one command action.
#[must_use]
pub fn command_iq(
    service: &Jid,
    node: &str,
    sessionid: Option<&str>,
    action: &str,
    payload: Option<Element>,
) -> Element {
    let mut command = Element::builder("command", NS_COMMANDS)
        .attr("node", node)
        .attr("action", action);
    if let Some(sessionid) = sessionid.filter(|sessionid| !sessionid.is_empty()) {
        command = command.attr("sessionid", sessionid);
    }
    if let Some(payload) = payload {
        command = command.append(payload);
    }
    Element::builder("iq", xmpp_parsers::ns::JABBER_CLIENT)
        .attr("type", "set")
        .attr("to", service.to_bare().to_string())
        .attr("id", format!("adhoc-exec-{}", uuid::Uuid::new_v4()))
        .append(command.build())
        .build()
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
    fn parses_a_single_stage_completed_reply() {
        let command = Element::from_str(
            r#"<command xmlns="http://jabber.org/protocol/commands" node="n" status="completed" sessionid="s1"><note type="info">done</note></command>"#,
        )
        .unwrap();
        let response = parse_response(&command);
        assert_eq!(response.status, AdHocStatus::Completed);
        assert_eq!(response.sessionid.as_deref(), Some("s1"));
        assert_eq!(
            response.notes,
            vec![AdHocNote {
                kind: "info".to_string(),
                text: "done".to_string()
            }]
        );
        assert!(response.actions.is_none());
        assert_eq!(response.default_action(), "complete");
        assert!(response.is_finished());
    }

    #[test]
    fn parses_a_multi_stage_reply() {
        let command = Element::from_str(
            r#"<command xmlns="http://jabber.org/protocol/commands" node="config" sessionid="s2" status="executing">
                 <actions execute="complete"><prev/><next/><complete/></actions>
                 <x xmlns="jabber:x:data" type="form">
                   <title>Configure Service</title>
                   <field var="service" type="list-single"><value>httpd</value></field>
                 </x>
               </command>"#,
        )
        .unwrap();
        let response = parse_response(&command);
        assert_eq!(response.status, AdHocStatus::Executing);
        assert!(!response.is_finished());
        assert_eq!(response.sessionid.as_deref(), Some("s2"));
        let actions = response.actions.as_ref().expect("actions");
        assert!(actions.prev);
        assert!(actions.next);
        assert!(actions.complete);
        assert_eq!(response.default_action(), "complete");
        let form = response.form.as_ref().expect("form");
        assert_eq!(form.fields.len(), 1);
        assert_eq!(form.fields[0].var, "service");
    }

    #[test]
    fn defaults_to_next_without_an_execute_attribute() {
        let command = Element::from_str(
            r#"<command xmlns="http://jabber.org/protocol/commands" node="n" status="executing"><actions><next/></actions></command>"#,
        )
        .unwrap();
        let response = parse_response(&command);
        assert_eq!(response.default_action(), "next");
    }

    #[test]
    fn parses_canceled_and_payload() {
        let command = Element::from_str(
            r#"<command xmlns="http://jabber.org/protocol/commands" node="n" status="canceled"><payload><x xmlns="urn:example"/></payload></command>"#,
        )
        .unwrap();
        let response = parse_response(&command);
        assert_eq!(response.status, AdHocStatus::Canceled);
        assert!(response.is_finished());
        assert!(response.payload.is_some());
    }

    #[test]
    fn builds_a_command_request() {
        let service: Jid = "example.org".parse().expect("service");
        let form = Element::from_str(
            r#"<x xmlns="jabber:x:data" type="submit"><field var="a"><value>b</value></field></x>"#,
        )
        .unwrap();
        let iq = command_iq(&service, "config", Some("s3"), "next", Some(form));
        let command = iq.get_child("command", NS_COMMANDS).expect("command");
        assert_eq!(command.attr("node"), Some("config"));
        assert_eq!(command.attr("action"), Some("next"));
        assert_eq!(command.attr("sessionid"), Some("s3"));
        assert!(command.get_child("x", NS_DATA_FORMS).is_some());
    }
}
