//! In-band registration (XEP-0077) on a fresh, unauthenticated stream.

use futures::{SinkExt, StreamExt};
use jid::Jid;
use minidom::Element;
use tokio_xmpp::connect::ServerConnector;
use tokio_xmpp::minidom::Element as TokioElement;
use tokio_xmpp::parsers::ns;
use tokio_xmpp::xmpp_stream::XMPPStream;
use tokio_xmpp::Packet;

use crate::XmppError;

const NS_REGISTER: &str = "jabber:iq:register";

fn iq(id: &str, kind: &str, payload: Element) -> TokioElement {
    Element::builder("iq", ns::JABBER_CLIENT)
        .attr("type", kind)
        .attr("id", id)
        .append(payload)
        .build()
}

async fn next_iq<S>(stream: &mut XMPPStream<S>, id: &str) -> Result<TokioElement, XmppError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        match stream.next().await {
            Some(Ok(Packet::Stanza(stanza)))
                if stanza.name() == "iq" && stanza.attr("id") == Some(id) =>
            {
                if stanza.attr("type") == Some("error") {
                    let message = stanza
                        .get_child("error", minidom::NSChoice::Any)
                        .and_then(|error| error.get_child("text", minidom::NSChoice::Any))
                        .map(minidom::Element::text)
                        .filter(|text| !text.trim().is_empty())
                        .unwrap_or_else(|| "registration was rejected".to_string());
                    return Err(XmppError::RegistrationFailed(message));
                }
                return Ok(stanza);
            }
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(XmppError::StreamError(error.to_string())),
            None => {
                return Err(XmppError::Disconnected(
                    "server closed the registration stream".into(),
                ))
            }
        }
    }
}

/// Register `username` on `server` with `password` (XEP-0077).
pub async fn register(
    connector: &super::connector::Connector,
    server: &str,
    username: &str,
    password: &str,
) -> Result<(), XmppError> {
    let jid: Jid = format!("{username}@{server}")
        .parse()
        .map_err(|error: jid::Error| XmppError::InvalidJid(error.to_string()))?;
    let mut stream = connector
        .connect(&jid, ns::JABBER_CLIENT)
        .await
        .map_err(|error| XmppError::ConnectionFailed(error.to_string()))?;

    stream
        .send(Packet::Stanza(iq(
            "reg-get",
            "get",
            Element::builder("query", NS_REGISTER).build(),
        )))
        .await
        .map_err(|error| XmppError::SendFailed(error.to_string()))?;
    next_iq(&mut stream, "reg-get").await?;

    let payload = Element::builder("query", NS_REGISTER)
        .append(Element::builder("username", NS_REGISTER).append(username.to_string()))
        .append(Element::builder("password", NS_REGISTER).append(password.to_string()))
        .build();
    stream
        .send(Packet::Stanza(iq("reg-set", "set", payload)))
        .await
        .map_err(|error| XmppError::SendFailed(error.to_string()))?;
    next_iq(&mut stream, "reg-set").await?;
    Ok(())
}
