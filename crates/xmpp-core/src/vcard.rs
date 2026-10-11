//! vCard-temp (XEP-0054) helpers: read-merge-write without losing fields.

use minidom::Element;

#[must_use]
fn field_key(element: &Element) -> (String, Option<String>) {
    (
        element.name().to_string(),
        element.attr("type").map(str::to_string),
    )
}

/// Merge `patch` into `existing`, replacing fields with the same name/type.
#[must_use]
pub fn merge_vcard(existing: Option<&Element>, patch: &Element) -> Element {
    let mut merged = Element::builder("vCard", "vcard-temp");
    let mut fields: Vec<Element> = Vec::new();

    if let Some(existing) = existing {
        fields.extend(existing.children().cloned());
    }

    for incoming in patch.children() {
        let key = field_key(incoming);
        fields.retain(|field| field_key(field) != key);
        fields.push(incoming.clone());
    }

    for field in fields {
        merged = merged.append(field);
    }
    merged.build()
}

/// Serialize an element for storage.
#[must_use]
pub fn to_xml(element: &Element) -> String {
    let mut buffer = Vec::new();
    element.write_to(&mut buffer).unwrap_or_default();
    String::from_utf8(buffer).unwrap_or_default()
}

/// Read the text of a direct child field, if present.
#[must_use]
pub fn field<'a>(vcard: &'a Element, name: &str) -> Option<&'a Element> {
    vcard.children().find(|child| child.name() == name)
}

/// Read the text of a direct child field.
#[must_use]
pub fn field_text(vcard: &Element, name: &str) -> Option<String> {
    let field = field(vcard, name)?;
    let nested = match name {
        "EMAIL" => field_text(field, "USERID"),
        "TEL" => field_text(field, "NUMBER"),
        "ADR" => field_text(field, "LOCALITY"),
        _ => None,
    };
    nested.or_else(|| {
        let text = field.text();
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_string())
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn merge_keeps_unpatched_fields_and_replaces_matching_ones() {
        let existing = Element::from_str(
            r#"<vCard xmlns="vcard-temp"><FN>Alice</FN><ORG>Acme</ORG><EMAIL><USERID>a@x</USERID></EMAIL></vCard>"#,
        )
        .unwrap();
        let patch = Element::from_str(
            r#"<vCard xmlns="vcard-temp"><FN>Alice B</FN><NICKNAME>ali</NICKNAME></vCard>"#,
        )
        .unwrap();

        let merged = merge_vcard(Some(&existing), &patch);
        assert_eq!(field_text(&merged, "FN").as_deref(), Some("Alice B"));
        assert_eq!(field_text(&merged, "ORG").as_deref(), Some("Acme"));
        assert_eq!(field_text(&merged, "NICKNAME").as_deref(), Some("ali"));
    }

    #[test]
    fn merge_without_existing_keeps_every_patch_field() {
        let patch = Element::from_str(
            r#"<vCard xmlns="vcard-temp"><FN>Bob</FN><TITLE>Dev</TITLE></vCard>"#,
        )
        .unwrap();
        let merged = merge_vcard(None, &patch);
        assert_eq!(field_text(&merged, "FN").as_deref(), Some("Bob"));
        assert_eq!(field_text(&merged, "TITLE").as_deref(), Some("Dev"));
    }
}
