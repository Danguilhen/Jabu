//! PEP user activity payloads (XEP-0080, XEP-0107, XEP-0108, XEP-0118, XEP-0292).

use minidom::Element;

pub const NS_TUNE: &str = "http://jabber.org/protocol/tune";
pub const NS_MOOD: &str = "http://jabber.org/protocol/mood";
pub const NS_ACTIVITY: &str = "http://jabber.org/protocol/activity";
pub const NS_LOCATION: &str = "http://jabber.org/protocol/geoloc";
pub const NS_VCARD4: &str = "urn:ietf:params:xml:ns:vcard-4.0";
pub const NS_VCARD4_NODE: &str = "urn:xmpp:vcard4";

/// Build a XEP-0107 tune payload.
#[must_use]
pub fn tune(
    artist: Option<&str>,
    title: Option<&str>,
    source: Option<&str>,
    length: Option<u32>,
) -> Element {
    let mut element = Element::builder("tune", NS_TUNE);
    for (name, value) in [("artist", artist), ("title", title), ("source", source)] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            element = element.append(Element::builder(name, NS_TUNE).append(value));
        }
    }
    if let Some(length) = length {
        element = element.append(Element::builder("length", NS_TUNE).append(length.to_string()));
    }
    element.build()
}

/// Build a XEP-0118 mood payload.
#[must_use]
pub fn mood(value: &str, text: Option<&str>) -> Element {
    let mut element = Element::builder("mood", NS_MOOD);
    if !value.is_empty() {
        element = element.append(Element::builder(value, NS_MOOD));
    }
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        element = element.append(Element::builder("text", NS_MOOD).append(text));
    }
    element.build()
}

/// Build a XEP-0108 activity payload.
#[must_use]
pub fn activity(general: &str, specific: Option<&str>, text: Option<&str>) -> Element {
    let mut element = Element::builder("activity", NS_ACTIVITY);
    if !general.is_empty() {
        let mut general_element = Element::builder(general, NS_ACTIVITY);
        if let Some(specific) = specific.filter(|specific| !specific.is_empty()) {
            general_element = general_element.append(Element::builder(specific, NS_ACTIVITY));
        }
        element = element.append(general_element);
    }
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        element = element.append(Element::builder("text", NS_ACTIVITY).append(text));
    }
    element.build()
}

/// Build a XEP-0080 location payload.
#[must_use]
pub fn location(latitude: f64, longitude: f64, text: Option<&str>) -> Element {
    let mut element = Element::builder("geoloc", NS_LOCATION)
        .append(Element::builder("lat", NS_LOCATION).append(format!("{latitude}")))
        .append(Element::builder("lon", NS_LOCATION).append(format!("{longitude}")));
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        element = element.append(Element::builder("text", NS_LOCATION).append(text));
    }
    element.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tune_carries_metadata() {
        let tune = tune(Some("Artist"), Some("Title"), None, Some(180));
        assert_eq!(tune.name(), "tune");
        assert_eq!(tune.get_child("artist", NS_TUNE).unwrap().text(), "Artist");
        assert_eq!(tune.get_child("length", NS_TUNE).unwrap().text(), "180");
    }

    #[test]
    fn mood_and_activity_nest_the_value() {
        let mood = mood("happy", Some("sunny"));
        assert!(mood.get_child("happy", NS_MOOD).is_some());
        assert_eq!(mood.get_child("text", NS_MOOD).unwrap().text(), "sunny");

        let activity = activity("working", Some("coding"), None);
        let working = activity.get_child("working", NS_ACTIVITY).unwrap();
        assert!(working.get_child("coding", NS_ACTIVITY).is_some());
    }

    #[test]
    fn location_uses_geoloc_fields() {
        let location = location(-23.5, -46.6, Some("Sao Paulo"));
        assert_eq!(
            location.get_child("lat", NS_LOCATION).unwrap().text(),
            "-23.5"
        );
        assert_eq!(
            location.get_child("lon", NS_LOCATION).unwrap().text(),
            "-46.6"
        );
    }
}
