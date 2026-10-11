//! vCard4 (XEP-0292) parsing, field mapping and read-merge-write.

use minidom::Element;

pub const NS_VCARD4: &str = "urn:ietf:params:xml:ns:vcard-4.0";

/// A flat view of the vCard4 fields the client displays and edits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VCard4 {
    pub full_name: Option<String>,
    pub nickname: Option<String>,
    pub organization: Option<String>,
    pub title: Option<String>,
    pub role: Option<String>,
    pub note: Option<String>,
    pub url: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub birthday: Option<String>,
    pub locality: Option<String>,
    pub country: Option<String>,
}

impl VCard4 {
    /// Parse a vCard4 payload; returns `None` when the XML is invalid.
    #[must_use]
    pub fn parse(xml: &str) -> Option<Self> {
        let element = xml.parse::<Element>().ok()?;
        Some(Self::from_element(&element))
    }

    /// Read the fields from an already parsed payload.
    #[must_use]
    pub fn from_element(element: &Element) -> Self {
        let adr = child(element, "adr");
        Self {
            full_name: field_value(element, "fn"),
            nickname: field_value(element, "nickname"),
            organization: field_value(element, "org"),
            title: field_value(element, "title"),
            role: field_value(element, "role"),
            note: field_value(element, "note"),
            url: field_value(element, "url"),
            email: field_value(element, "email"),
            phone: field_value(element, "tel"),
            birthday: field_value(element, "bday"),
            locality: adr.and_then(|adr| text(adr, "locality")),
            country: adr.and_then(|adr| text(adr, "country")),
        }
    }

    /// Read a vCard-temp-style field name (`FN`, `ORG`, `EMAIL`, …).
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        match name.to_ascii_uppercase().as_str() {
            "FN" => self.full_name.as_deref(),
            "NICKNAME" => self.nickname.as_deref(),
            "ORG" => self.organization.as_deref(),
            "TITLE" => self.title.as_deref(),
            "ROLE" => self.role.as_deref(),
            "NOTE" => self.note.as_deref(),
            "URL" => self.url.as_deref(),
            "EMAIL" => self.email.as_deref(),
            "TEL" => self.phone.as_deref(),
            "BDAY" => self.birthday.as_deref(),
            "ADR" => self.locality.as_deref(),
            _ => None,
        }
    }

    /// Build a vCard4 payload from the set fields.
    #[must_use]
    pub fn to_element(&self) -> Element {
        let mut element = Element::builder("vcard", NS_VCARD4);
        if let Some(value) = self.full_name.as_deref() {
            element = element.append(field_leaf("fn", value));
        }
        if let Some(value) = self.nickname.as_deref() {
            element = element.append(field_leaf("nickname", value));
        }
        if let Some(value) = self.organization.as_deref() {
            element =
                element.append(Element::builder("org", NS_VCARD4).append(leaf("text", value)));
        }
        if let Some(value) = self.title.as_deref() {
            element = element.append(field_leaf("title", value));
        }
        if let Some(value) = self.role.as_deref() {
            element = element.append(field_leaf("role", value));
        }
        if let Some(value) = self.note.as_deref() {
            element = element.append(field_leaf("note", value));
        }
        if let Some(value) = self.url.as_deref() {
            element = element.append(field_leaf("url", value));
        }
        if let Some(value) = self.email.as_deref() {
            element =
                element.append(Element::builder("email", NS_VCARD4).append(leaf("text", value)));
        }
        if let Some(value) = self.phone.as_deref() {
            element =
                element.append(Element::builder("tel", NS_VCARD4).append(leaf("text", value)));
        }
        if let Some(value) = self.birthday.as_deref() {
            element = element.append(field_leaf("bday", value));
        }
        element.build()
    }

    /// Build a vCard-temp payload with the same fields, for XEP-0054 servers.
    #[must_use]
    pub fn to_vcard_temp(&self) -> Element {
        let ns = "vcard-temp";
        let mut element = Element::builder("vCard", ns);
        if let Some(value) = self.full_name.as_deref() {
            element = element.append(Element::builder("FN", ns).append(value.trim()));
        }
        if let Some(value) = self.nickname.as_deref() {
            element = element.append(Element::builder("NICKNAME", ns).append(value.trim()));
        }
        if let Some(value) = self.organization.as_deref() {
            element = element.append(Element::builder("ORG", ns).append(value.trim()));
        }
        if let Some(value) = self.title.as_deref() {
            element = element.append(Element::builder("TITLE", ns).append(value.trim()));
        }
        if let Some(value) = self.role.as_deref() {
            element = element.append(Element::builder("ROLE", ns).append(value.trim()));
        }
        if let Some(value) = self.note.as_deref() {
            element = element.append(Element::builder("NOTE", ns).append(value.trim()));
        }
        if let Some(value) = self.url.as_deref() {
            element = element.append(Element::builder("URL", ns).append(value.trim()));
        }
        if let Some(value) = self.email.as_deref() {
            element = element.append(
                Element::builder("EMAIL", ns)
                    .append(Element::builder("USERID", ns).append(value.trim())),
            );
        }
        if let Some(value) = self.phone.as_deref() {
            element = element.append(
                Element::builder("TEL", ns)
                    .append(Element::builder("NUMBER", ns).append(value.trim())),
            );
        }
        if let Some(value) = self.birthday.as_deref() {
            element = element.append(Element::builder("BDAY", ns).append(value.trim()));
        }
        if self.locality.is_some() || self.country.is_some() {
            let mut adr = Element::builder("ADR", ns);
            if let Some(value) = self.locality.as_deref() {
                adr = adr.append(Element::builder("LOCALITY", ns).append(value.trim()));
            }
            if let Some(value) = self.country.as_deref() {
                adr = adr.append(Element::builder("CTRY", ns).append(value.trim()));
            }
            element = element.append(adr);
        }
        element.build()
    }
}

/// Read a vCard4 field from XML using the vCard-temp field name.
#[must_use]
pub fn field_text(xml: &str, name: &str) -> Option<String> {
    let vcard = VCard4::parse(xml)?;
    vcard.field(name).map(str::to_string)
}

/// Merge `patch` into `existing`, replacing fields with the same name/type.
#[must_use]
pub fn merge_vcard4(existing: Option<&Element>, patch: &Element) -> Element {
    let mut merged = Element::builder("vcard", NS_VCARD4);
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

fn field_key(element: &Element) -> (String, Option<String>, Option<String>) {
    (
        element.name().to_string(),
        element.attr("type").map(str::to_string),
        element.attr("pref").map(str::to_string),
    )
}

fn field_leaf(name: &str, value: &str) -> Element {
    Element::builder(name, NS_VCARD4)
        .append(leaf("text", value))
        .build()
}

fn leaf(name: &str, value: &str) -> Element {
    Element::builder(name, NS_VCARD4)
        .append(value.trim())
        .build()
}

fn child<'a>(element: &'a Element, name: &str) -> Option<&'a Element> {
    element.children().find(|child| child.name() == name)
}

fn text(element: &Element, name: &str) -> Option<String> {
    child(element, name).and_then(|child| non_empty(child.text()))
}

fn field_value(element: &Element, name: &str) -> Option<String> {
    let field = child(element, name)?;
    text(field, "text")
        .or_else(|| text(field, "date"))
        .or_else(|| non_empty(field.text()))
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn parses_the_fields_the_ui_shows() {
        let xml = r#"<vcard xmlns="urn:ietf:params:xml:ns:vcard-4.0"><fn><text>Alice B</text></fn><nickname><text>ali</text></nickname><org><text>Acme</text></org><title><text>Dev</text></title><email><text>alice@example.org</text></email><tel><text>+1 555</text></tel><url><text>https://example.org</text></url><note><text>Hi</text></note><adr><locality>Berlin</locality><country>DE</country></adr></vcard>"#;
        let vcard = VCard4::parse(xml).unwrap();
        assert_eq!(vcard.full_name.as_deref(), Some("Alice B"));
        assert_eq!(vcard.organization.as_deref(), Some("Acme"));
        assert_eq!(vcard.email.as_deref(), Some("alice@example.org"));
        assert_eq!(vcard.phone.as_deref(), Some("+1 555"));
        assert_eq!(vcard.locality.as_deref(), Some("Berlin"));
        assert_eq!(vcard.field("FN"), Some("Alice B"));
        assert_eq!(vcard.field("ORG"), Some("Acme"));
    }

    #[test]
    fn merge_keeps_unpatched_fields_and_replaces_matching_ones() {
        let existing = Element::from_str(
            r#"<vcard xmlns="urn:ietf:params:xml:ns:vcard-4.0"><fn><text>Alice</text></fn><org><text>Acme</text></org><email><text>a@x</text></email></vcard>"#,
        )
        .unwrap();
        let patch = Element::from_str(
            r#"<vcard xmlns="urn:ietf:params:xml:ns:vcard-4.0"><fn><text>Alice B</text></fn><nickname><text>ali</text></nickname></vcard>"#,
        )
        .unwrap();

        let merged = merge_vcard4(Some(&existing), &patch);
        let vcard = VCard4::from_element(&merged);
        assert_eq!(vcard.full_name.as_deref(), Some("Alice B"));
        assert_eq!(vcard.organization.as_deref(), Some("Acme"));
        assert_eq!(vcard.email.as_deref(), Some("a@x"));
        assert_eq!(vcard.nickname.as_deref(), Some("ali"));
    }

    #[test]
    fn built_element_round_trips_through_the_parser() {
        let original = VCard4 {
            full_name: Some("Bob".into()),
            organization: Some("Acme".into()),
            email: Some("bob@example.org".into()),
            phone: Some("+1 555".into()),
            ..VCard4::default()
        };
        let parsed = VCard4::from_element(&original.to_element());
        assert_eq!(parsed, original);
    }
}
