//! XEP-0100 gateway discovery and registration forms.

use minidom::Element;

pub const NS_REGISTER: &str = "jabber:iq:register";
pub const NS_DISCO_ITEMS: &str = "http://jabber.org/protocol/disco#items";
pub const NS_DISCO_INFO: &str = "http://jabber.org/protocol/disco#info";
pub const NS_DATA_FORMS: &str = "jabber:x:data";

/// A gateway component advertised by the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gateway {
    pub jid: String,
    pub name: String,
    pub kind: String,
    pub description: Option<String>,
}

/// One field in a gateway registration form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayField {
    pub var: String,
    pub label: String,
    pub kind: String,
    pub value: String,
    pub options: Vec<(String, String)>,
    pub required: bool,
}

impl GatewayField {
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.kind == "hidden"
    }

    #[must_use]
    pub fn is_secret(&self) -> bool {
        self.kind == "password"
            || self.var.eq_ignore_ascii_case("password")
            || self.var.eq_ignore_ascii_case("secret")
    }
}

/// The registration form returned by a gateway.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GatewayRegistration {
    pub registered: bool,
    pub instructions: Option<String>,
    pub fields: Vec<GatewayField>,
    pub form: Option<Element>,
}

/// Read a gateway from its `disco#info` reply.
#[must_use]
pub fn parse_gateway(jid: &str, info: &Element, fallback_name: Option<&str>) -> Option<Gateway> {
    let identity = info
        .children()
        .find(|child| child.name() == "identity" && child.attr("category") == Some("gateway"))?;
    let kind = identity.attr("type").unwrap_or("gateway").to_string();
    let name = identity
        .attr("name")
        .or(fallback_name)
        .map(str::to_string)
        .unwrap_or_else(|| jid.to_string());
    Some(Gateway {
        jid: jid.to_string(),
        name,
        kind,
        description: info
            .children()
            .find(|child| child.name() == "identity")
            .and_then(|identity| identity.attr("name"))
            .map(str::to_string),
    })
}

/// Parse a `jabber:iq:register` reply.
#[must_use]
pub fn parse_registration(query: &Element) -> GatewayRegistration {
    let registered = query.children().any(|child| child.name() == "registered");
    let instructions = query
        .children()
        .find(|child| child.name() == "instructions")
        .map(Element::text)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());

    let form = query
        .get_child("x", NS_DATA_FORMS)
        .filter(|form| form.attr("type") != Some("result"));
    let fields = if let Some(form) = form {
        form.children()
            .filter(|child| child.name() == "field")
            .filter_map(parse_field)
            .collect()
    } else {
        legacy_fields(query)
    };

    GatewayRegistration {
        registered,
        instructions,
        fields,
        form: form.cloned(),
    }
}

fn parse_field(field: &Element) -> Option<GatewayField> {
    let var = field.attr("var")?.to_string();
    let kind = field.attr("type").unwrap_or("text").to_string();
    let label = field
        .attr("label")
        .map(str::to_string)
        .unwrap_or_else(|| var.clone());
    let value = field
        .children()
        .find(|child| child.name() == "value")
        .map(Element::text)
        .unwrap_or_default();
    let options = field
        .children()
        .filter(|child| child.name() == "option")
        .map(|option| {
            let label = option.attr("label").map(str::to_string).unwrap_or_default();
            let value = option
                .children()
                .find(|child| child.name() == "value")
                .map(Element::text)
                .unwrap_or_default();
            (label, value)
        })
        .collect();
    let required = field.children().any(|child| child.name() == "required");
    Some(GatewayField {
        var,
        label,
        kind,
        value,
        options,
        required,
    })
}

fn legacy_fields(query: &Element) -> Vec<GatewayField> {
    query
        .children()
        .filter(|child| child.name() != "instructions" && child.name() != "registered")
        .filter(|child| !child.name().starts_with('x'))
        .map(|child| {
            let var = child.name().to_string();
            let kind = if var.eq_ignore_ascii_case("password") {
                "password"
            } else {
                "text"
            };
            GatewayField {
                label: var.clone(),
                var,
                kind: kind.to_string(),
                value: child.text().trim().to_string(),
                options: Vec::new(),
                required: false,
            }
        })
        .collect()
}

/// Build the `jabber:iq:register` payload for a registration or removal.
#[must_use]
pub fn registration_payload(
    form: Option<&Element>,
    values: &[(String, String)],
    remove: bool,
) -> Element {
    let mut query = Element::builder("query", NS_REGISTER);
    if remove {
        return query
            .append(Element::builder("remove", NS_REGISTER))
            .build();
    }
    if let Some(form) = form {
        let mut submit = Element::builder("x", NS_DATA_FORMS).attr("type", "submit");
        for field in form.children().filter(|child| child.name() == "field") {
            let var = field.attr("var").unwrap_or_default();
            let hidden = field.attr("type") == Some("hidden");
            let value = values
                .iter()
                .find(|(name, _)| name == var)
                .map(|(_, value)| value.clone())
                .or_else(|| {
                    field
                        .children()
                        .find(|child| child.name() == "value")
                        .map(Element::text)
                });
            if hidden || value.is_some() {
                let mut out = Element::builder("field", NS_DATA_FORMS).attr("var", var);
                if let Some(value) = value {
                    out = out.append(Element::builder("value", NS_DATA_FORMS).append(value));
                }
                submit = submit.append(out.build());
            }
        }
        query = query.append(submit.build());
    } else {
        for (var, value) in values {
            if value.trim().is_empty() {
                continue;
            }
            query = query.append(Element::builder(var.as_str(), NS_REGISTER).append(value.trim()));
        }
    }
    query.build()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn recognises_gateway_identities() {
        let info = Element::from_str(
            r#"<query xmlns="http://jabber.org/protocol/disco#info"><identity category="gateway" type="irc" name="IRC"/><feature var="jabber:iq:register"/></query>"#,
        )
        .unwrap();
        let gateway = parse_gateway("irc.example.org", &info, None).unwrap();
        assert_eq!(gateway.kind, "irc");
        assert_eq!(gateway.name, "IRC");
    }

    #[test]
    fn parses_a_data_form_registration() {
        let query = Element::from_str(
            r#"<query xmlns="jabber:iq:register"><instructions>Use your network account</instructions><x xmlns="jabber:x:data" type="form"><field var="username" type="text-single" label="Username"><required/></field><field var="password" type="text-private" label="Password"/><field var="network" type="list-single" label="Network"><option label="Libera"><value>libera</value></option></field></x></query>"#,
        )
        .unwrap();
        let form = parse_registration(&query);
        assert!(!form.registered);
        assert_eq!(
            form.instructions.as_deref(),
            Some("Use your network account")
        );
        assert_eq!(form.fields.len(), 3);
        assert!(form.fields[0].required);
        assert!(form.fields[1].is_secret());
        assert_eq!(form.fields[2].options[0].1, "libera");
    }

    #[test]
    fn parses_and_builds_the_legacy_form() {
        let query = Element::from_str(
            r#"<query xmlns="jabber:iq:register"><registered/><username>bob</username><password>secret</password></query>"#,
        )
        .unwrap();
        let form = parse_registration(&query);
        assert!(form.registered);
        assert_eq!(form.fields.len(), 2);

        let payload = registration_payload(None, &[("username".into(), "alice".into())], false);
        assert_eq!(
            payload.get_child("username", NS_REGISTER).unwrap().text(),
            "alice"
        );
    }

    #[test]
    fn builds_a_submit_form_with_hidden_fields() {
        let query = Element::from_str(
            r#"<query xmlns="jabber:iq:register"><x xmlns="jabber:x:data" type="form"><field var="token" type="hidden"><value>abc</value></field><field var="username" type="text-single"/></x></query>"#,
        )
        .unwrap();
        let form = parse_registration(&query);
        let payload = registration_payload(
            form.form.as_ref(),
            &[("username".into(), "alice".into())],
            false,
        );
        let submit = payload.get_child("x", NS_DATA_FORMS).unwrap();
        assert_eq!(submit.attr("type"), Some("submit"));
        let token = submit
            .children()
            .find(|field| field.attr("var") == Some("token"))
            .unwrap();
        assert_eq!(
            token.get_child("value", NS_DATA_FORMS).unwrap().text(),
            "abc"
        );
    }
}
