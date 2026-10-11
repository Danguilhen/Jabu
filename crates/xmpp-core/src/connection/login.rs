//! SASL login plus XEP-0198 resumption or resource binding.

use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

use futures::StreamExt;
use minidom::Element;
use sasl::client::mechanisms::{Anonymous, Plain, Scram};
use sasl::client::Mechanism;
use sasl::common::scram::{Sha1, Sha256};
use sasl::common::{ChannelBinding, Credentials};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_xmpp::connect::ServerConnector;
use tokio_xmpp::parsers::bind::{BindQuery, BindResponse};
use tokio_xmpp::parsers::iq::{Iq, IqType};
use tokio_xmpp::parsers::ns;
use tokio_xmpp::parsers::sasl::{
    Auth, Challenge, Failure, Mechanism as SaslMechanism, Response, Success,
};
use tokio_xmpp::xmpp_stream::XMPPStream;
use tokio_xmpp::Packet;

use super::connector::{ClientStream, Connector, ConnectorError};
use crate::StreamManagement;

const SM_NS: &str = "urn:xmpp:sm:3";
const BIND_ID: &str = "resource-bind";

/// Result of a successful login.
pub struct Login {
    pub stream: ClientStream,
    pub resumed: bool,
}

/// Connect, authenticate and either resume the old stream or bind a new one.
pub async fn login(
    connector: Connector,
    jid: jid::Jid,
    password: String,
    sm: Option<Arc<StreamManagement>>,
) -> Result<Login, ConnectorError> {
    let stream = connector.connect(&jid, ns::JABBER_CLIENT).await?;
    let stream = authenticate(stream, &jid, &password).await?;

    let mut stream = XMPPStream::start(stream, jid, ns::JABBER_CLIENT.to_owned()).await?;

    if let Some(sm) = sm.as_deref() {
        if supports_stream_management(&stream) {
            if let Some((previd, handled)) = sm.resume_request() {
                if try_resume(&mut stream, sm, &previd, handled).await? {
                    return Ok(Login {
                        stream,
                        resumed: true,
                    });
                }
            }
        }
    }

    let mut stream = bind(stream).await?;
    if let Some(sm) = sm.as_deref() {
        let unacked = sm.take_unacked_stanzas();
        for stanza in unacked {
            sm.note_sent_stanza(stanza.clone());
            stream.send_stanza(stanza).await?;
        }
    }

    Ok(Login {
        stream,
        resumed: false,
    })
}

fn supports_stream_management<S>(stream: &XMPPStream<S>) -> bool
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream.stream_features.0.get_child("sm", SM_NS).is_some()
}

async fn try_resume<S>(
    stream: &mut XMPPStream<S>,
    sm: &StreamManagement,
    previd: &str,
    handled: u64,
) -> Result<bool, ConnectorError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream
        .send_stanza(
            Element::builder("resume", SM_NS)
                .attr("previd", previd.to_string())
                .attr("h", handled.to_string())
                .build(),
        )
        .await?;

    loop {
        match stream.next().await {
            Some(Ok(Packet::Stanza(stanza))) if stanza.is("resumed", SM_NS) => {
                let h = stanza
                    .attr("h")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
                sm.resume_ok(h);

                let unacked = sm.take_unacked_stanzas();
                for pending in unacked {
                    sm.note_sent_stanza(pending.clone());
                    stream.send_stanza(pending).await?;
                }
                return Ok(true);
            }
            Some(Ok(Packet::Stanza(stanza))) if stanza.is("failed", SM_NS) => {
                sm.clear_resumption();
                return Ok(false);
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(e.into()),
            None => return Err(ConnectorError::Disconnected),
        }
    }
}

async fn bind<S>(mut stream: XMPPStream<S>) -> Result<XMPPStream<S>, ConnectorError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if !stream.stream_features.can_bind() {
        return Ok(stream);
    }

    let resource = stream.jid.resource().map(|resource| resource.to_string());
    stream
        .send_stanza(Iq::from_set(BIND_ID, BindQuery::new(resource)))
        .await?;

    loop {
        match stream.next().await {
            Some(Ok(Packet::Stanza(stanza))) => {
                let Ok(iq) = Iq::try_from(stanza) else {
                    continue;
                };
                if iq.id != BIND_ID {
                    continue;
                }
                match iq.payload {
                    IqType::Result(payload) => {
                        if let Some(payload) = payload {
                            if let Ok(bind) = BindResponse::try_from(payload) {
                                stream.jid = bind.into();
                            }
                        }
                        return Ok(stream);
                    }
                    _ => return Err(ConnectorError::InvalidBindResponse),
                }
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(e.into()),
            None => return Err(ConnectorError::Disconnected),
        }
    }
}

async fn authenticate<S>(
    mut stream: XMPPStream<S>,
    jid: &jid::Jid,
    password: &str,
) -> Result<S, ConnectorError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let username = jid
        .node()
        .map(|node| node.as_str().to_string())
        .ok_or_else(|| ConnectorError::Sasl("JID has no localpart".to_string()))?;

    let creds = Credentials::default()
        .with_username(username)
        .with_password(password.to_string())
        .with_channel_binding(ChannelBinding::None);

    let remote: HashSet<String> = stream
        .stream_features
        .sasl_mechanisms()
        .map_err(|e| ConnectorError::Sasl(e.to_string()))?
        .collect();

    type LocalMechanism = Box<dyn Fn() -> Option<Box<dyn Mechanism + Send + Sync>> + Send>;
    let mechanisms: Vec<(&str, LocalMechanism)> = vec![
        (
            "SCRAM-SHA-256",
            Box::new({
                let creds = creds.clone();
                move || {
                    Scram::<Sha256>::from_credentials(creds.clone())
                        .ok()
                        .map(|m| Box::new(m) as Box<dyn Mechanism + Send + Sync>)
                }
            }),
        ),
        (
            "SCRAM-SHA-1",
            Box::new({
                let creds = creds.clone();
                move || {
                    Scram::<Sha1>::from_credentials(creds.clone())
                        .ok()
                        .map(|m| Box::new(m) as Box<dyn Mechanism + Send + Sync>)
                }
            }),
        ),
        (
            "PLAIN",
            Box::new({
                let creds = creds.clone();
                move || {
                    Plain::from_credentials(creds.clone())
                        .ok()
                        .map(|m| Box::new(m) as Box<dyn Mechanism + Send + Sync>)
                }
            }),
        ),
        (
            "ANONYMOUS",
            Box::new(|| Some(Box::new(Anonymous::new()) as Box<dyn Mechanism + Send + Sync>)),
        ),
    ];

    for (name, factory) in mechanisms {
        if !remote.contains(name) {
            continue;
        }
        let Some(mut mechanism) = factory() else {
            continue;
        };

        let name =
            SaslMechanism::from_str(name).map_err(|e| ConnectorError::Sasl(e.to_string()))?;
        stream
            .send_stanza(Auth {
                mechanism: name,
                data: mechanism.initial(),
            })
            .await?;

        loop {
            match stream.next().await {
                Some(Ok(Packet::Stanza(stanza))) => {
                    if let Ok(challenge) = Challenge::try_from(stanza.clone()) {
                        let response = mechanism
                            .response(&challenge.data)
                            .map_err(|e| ConnectorError::Sasl(e.to_string()))?;
                        stream.send_stanza(Response { data: response }).await?;
                    } else if Success::try_from(stanza.clone()).is_ok() {
                        return Ok(stream.into_inner());
                    } else if let Ok(failure) = Failure::try_from(stanza) {
                        return Err(ConnectorError::Sasl(format!(
                            "authentication failed: {:?}",
                            failure.defined_condition
                        )));
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(e.into()),
                None => return Err(ConnectorError::Disconnected),
            }
        }
    }

    Err(ConnectorError::Sasl(
        "no supported SASL mechanism offered".to_string(),
    ))
}
