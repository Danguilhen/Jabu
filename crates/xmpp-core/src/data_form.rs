//! Minimal XEP-0004 data-form parsing and submission.

use minidom::Element;

pub const NS_DATA_FORMS: &str = "jabber:x:data";

/// One field of a data form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    pub var: String,
    pub label: String,
    pub kind: String,
    pub value: String,
    pub options: Vec<(String, String)>,
    pub required: bool,
}

impl FormField {
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.kind == "hidden"
    }

    #[must_use]
    pub fn is_secret(&self) -> bool {
        self.kind == "password" || self.var.eq_ignore_ascii_case("password")
    }

    #[must_use]
    pub fn is_boolean(&self) -> bool {
        self.kind == "boolean"
    }
}

/// A parsed data form, keeping the original element for hidden fields.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DataForm {
    pub title: Option<String>,
    pub instructions: Option<String>,
    pub fields: Vec<FormField>,
    pub form: Option<Element>,
}

/// Parse a `<x xmlns='jabber:x:data'/>` form.
#[must_use]
pub fn parse(form: &Element) -> DataForm {
    let title = form
        .get_child("title", NS_DATA_FORMS)
        .map(Element::text)
        .filter(|title| !title.trim().is_empty());
    let instructions = form
        .get_child("instructions", NS_DATA_FORMS)
        .map(Element::text)
        .filter(|text| !text.trim().is_empty());
    let fields = form
        .children()
        .filter(|child| child.name() == "field")
        .filter_map(parse_field)
        .collect();
    DataForm {
        title,
        instructions,
        fields,
        form: Some(form.clone()),
    }
}

fn parse_field(field: &Element) -> Option<FormField> {
    let var = field.attr("var")?.to_string();
    let kind = field.attr("type").unwrap_or("text-single").to_string();
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
            let value = option
                .get_child("value", NS_DATA_FORMS)
                .map(Element::text)
                .unwrap_or_default();
            let label = option
                .attr("label")
                .map(str::to_string)
                .unwrap_or_else(|| value.clone());
            (label, value)
        })
        .collect();
    let required = field.children().any(|child| child.name() == "required");
    Some(FormField {
        var,
        label,
        kind,
        value,
        options,
        required,
    })
}

/// Build a submit form, preserving hidden fields and default values.
#[must_use]
pub fn submit(form: Option<&Element>, values: &[(String, String)]) -> Element {
    let mut submit = Element::builder("x", NS_DATA_FORMS).attr("type", "submit");
    if let Some(form) = form {
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
                if hidden {
                    out = out.attr("type", "hidden");
                }
                if let Some(value) = value {
                    out = out.append(Element::builder("value", NS_DATA_FORMS).append(value));
                }
                submit = submit.append(out.build());
            }
        }
    } else {
        for (var, value) in values {
            if value.trim().is_empty() {
                continue;
            }
            submit = submit.append(
                Element::builder("field", NS_DATA_FORMS)
                    .attr("var", var.as_str())
                    .append(Element::builder("value", NS_DATA_FORMS).append(value.trim())),
            );
        }
    }
    submit.build()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn parses_and_submits_a_form() {
        let form = Element::from_str(
            r#"<x xmlns="jabber:x:data" type="form"><title>Room</title><instructions>Edit it</instructions><field var="FORM_TYPE" type="hidden"><value>http://jabber.org/protocol/muc#roomconfig</value></field><field var="muc#roomconfig_roomname" type="text-single" label="Name"><value>Old</value></field><field var="muc#roomconfig_persistentroom" type="boolean" label="Persistent"><value>1</value></field></x>"#,
        )
        .unwrap();
        let parsed = parse(&form);
        assert_eq!(parsed.title.as_deref(), Some("Room"));
        assert_eq!(parsed.instructions.as_deref(), Some("Edit it"));
        assert_eq!(parsed.fields.len(), 3);
        assert!(parsed.fields[0].is_hidden());
        assert!(parsed.fields[2].is_boolean());

        let submit = submit(
            parsed.form.as_ref(),
            &[("muc#roomconfig_roomname".to_string(), "New".to_string())],
        );
        assert_eq!(submit.attr("type"), Some("submit"));
        let fields: Vec<_> = submit
            .children()
            .filter(|child| child.name() == "field")
            .collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].attr("type"), Some("hidden"));
        assert_eq!(
            fields[1]
                .get_child("value", NS_DATA_FORMS)
                .map(Element::text)
                .as_deref(),
            Some("New")
        );
    }
}
