//! XEP-0231 Bits of Binary helpers and process-local cache.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use minidom::Element;
use sha2::{Digest, Sha256};

pub const NS_BOB: &str = "urn:xmpp:bob";
const NS_XHTML_IM: &str = "http://jabber.org/protocol/xhtml-im";
const NS_XHTML: &str = "http://www.w3.org/1999/xhtml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BobContent {
    pub cid: String,
    pub mime: String,
    pub data: String,
    pub max_age: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BobRequest {
    pub id: String,
    pub from: String,
    pub cid: String,
}

static CACHE: OnceLock<Mutex<HashMap<String, BobContent>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, BobContent>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register(content: BobContent) {
    if let Ok(mut cache) = cache().lock() {
        cache.insert(content.cid.clone(), content);
    }
}

#[must_use]
pub fn get(cid: &str) -> Option<BobContent> {
    cache().lock().ok()?.get(cid).cloned()
}

#[must_use]
pub fn data_uri(cid: &str) -> Option<String> {
    let content = get(cid)?;
    Some(format!("data:{};base64,{}", content.mime, content.data))
}

#[must_use]
pub fn cid_for_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256+{hash}@bob.xmpp.org")
}

#[must_use]
pub fn parse_request(iq: &Element) -> Option<BobRequest> {
    if iq.attr("type") != Some("get") {
        return None;
    }
    let data = iq.get_child("data", NS_BOB)?;
    let cid = data.attr("cid")?.trim_start_matches("cid:");
    Some(BobRequest {
        id: iq.attr("id")?.to_string(),
        from: iq.attr("from")?.to_string(),
        cid: cid.to_string(),
    })
}

#[must_use]
pub fn parse_result(iq: &Element) -> Option<BobContent> {
    if iq.attr("type") != Some("result") {
        return None;
    }
    let data = iq.get_child("data", NS_BOB)?;
    let cid = data.attr("cid")?.trim_start_matches("cid:").to_string();
    let mime = data
        .attr("type")
        .unwrap_or("application/octet-stream")
        .to_string();
    let max_age = data
        .attr("max-age")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let body = data.text().trim().to_string();
    if body.is_empty() {
        return None;
    }
    Some(BobContent {
        cid,
        mime,
        data: body,
        max_age,
    })
}

#[must_use]
pub fn response(request: &BobRequest, content: &BobContent) -> Element {
    Element::builder("iq", "jabber:client")
        .attr("type", "result")
        .attr("id", request.id.clone())
        .attr("to", request.from.clone())
        .append(
            Element::builder("data", NS_BOB)
                .attr("cid", format!("cid:{}", content.cid))
                .attr("type", content.mime.clone())
                .attr("max-age", content.max_age.to_string())
                .append(content.data.clone()),
        )
        .build()
}

#[must_use]
pub fn request(iq_id: &str, to: &str, cid: &str) -> Element {
    Element::builder("iq", "jabber:client")
        .attr("type", "get")
        .attr("id", iq_id)
        .attr("to", to)
        .append(
            Element::builder("data", NS_BOB)
                .attr("cid", format!("cid:{}", cid.trim_start_matches("cid:"))),
        )
        .build()
}

#[must_use]
pub fn sticker_html(cid: &str, alt: &str) -> Element {
    Element::builder("html", NS_XHTML_IM)
        .append(
            Element::builder("body", NS_XHTML).append(
                Element::builder("p", NS_XHTML).append(
                    Element::builder("img", NS_XHTML)
                        .attr("src", format!("cid:{}", cid.trim_start_matches("cid:")))
                        .attr("alt", alt),
                ),
            ),
        )
        .build()
}

#[must_use]
pub fn parse_sticker(message: &Element) -> Option<(String, String)> {
    let html = message.get_child("html", NS_XHTML_IM)?;
    let img = find_img(html)?;
    let src = img.attr("src")?.trim_start_matches("cid:").to_string();
    let alt = img.attr("alt").unwrap_or("Sticker").to_string();
    Some((src, alt))
}

fn find_img(element: &Element) -> Option<&Element> {
    if element.name() == "img" {
        return Some(element);
    }
    element.children().find_map(|child| find_img(child))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn cid_is_sha256_prefixed() {
        let cid = cid_for_bytes(b"hello");
        assert!(cid.starts_with("sha256+"));
        assert!(cid.ends_with("@bob.xmpp.org"));
    }

    #[test]
    fn parses_sticker_html() {
        let message = Element::from_str(
            r#"<message xmlns="jabber:client"><html xmlns="http://jabber.org/protocol/xhtml-im"><body xmlns="http://www.w3.org/1999/xhtml"><p><img src="cid:sha256+abc@bob.xmpp.org" alt="Sticker"/></p></body></html></message>"#,
        )
        .unwrap();
        let (cid, alt) = parse_sticker(&message).unwrap();
        assert_eq!(cid, "sha256+abc@bob.xmpp.org");
        assert_eq!(alt, "Sticker");
    }

    #[test]
    fn builds_a_response_and_roundtrips_sticker_html() {
        let request = BobRequest {
            id: "b1".into(),
            from: "bob@example.org/res".into(),
            cid: "sha256+abc@bob.xmpp.org".into(),
        };
        let content = BobContent {
            cid: request.cid.clone(),
            mime: "image/png".into(),
            data: "aGVsbG8=".into(),
            max_age: 60,
        };
        let iq = response(&request, &content);
        assert_eq!(iq.attr("type"), Some("result"));
        let parsed = parse_result(&iq).unwrap();
        assert_eq!(parsed.cid, request.cid);
        assert_eq!(parsed.mime, "image/png");

        let html = sticker_html(&request.cid, "Party");
        let message = Element::builder("message", "jabber:client")
            .append(html)
            .build();
        let (cid, alt) = parse_sticker(&message).unwrap();
        assert_eq!(cid, request.cid);
        assert_eq!(alt, "Party");
    }

    #[test]
    fn registers_and_returns_data_uri() {
        let cid = cid_for_bytes(b"png");
        register(BobContent {
            cid: cid.clone(),
            mime: "image/png".into(),
            data: "aGVsbG8=".into(),
            max_age: 3600,
        });
        assert_eq!(
            data_uri(&cid).as_deref(),
            Some("data:image/png;base64,aGVsbG8=")
        );
    }
}
