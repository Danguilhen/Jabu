//! Typed IQ request routing with ids, timeouts and error mapping.

use std::collections::HashMap;
use std::time::Duration;

use minidom::Element;
use parking_lot::Mutex;
use tokio::sync::oneshot;

use crate::{Result, XmppError};

/// Why an IQ request did not return a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IqError {
    /// The server did not answer before the router's deadline.
    Timeout,
    /// The server answered with `type="error"`.
    Remote {
        kind: String,
        condition: Option<String>,
        text: Option<String>,
    },
    /// The connection went away while the request was in flight.
    Disconnected,
}

impl std::fmt::Display for IqError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => write!(formatter, "IQ request timed out"),
            Self::Disconnected => write!(formatter, "connection closed before the IQ reply"),
            Self::Remote {
                kind,
                condition,
                text,
            } => {
                write!(formatter, "IQ error ({kind})")?;
                if let Some(condition) = condition {
                    write!(formatter, ": {condition}")?;
                }
                if let Some(text) = text {
                    write!(formatter, " - {text}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for IqError {}

/// Matches IQ replies to the requests waiting for them.
pub struct IqRouter {
    pending: Mutex<HashMap<String, oneshot::Sender<std::result::Result<Element, IqError>>>>,
    timeout: Duration,
}

impl Default for IqRouter {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

impl IqRouter {
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            timeout,
        }
    }

    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Register interest in the reply to `id` before sending the request.
    pub fn register(&self, id: &str) -> oneshot::Receiver<std::result::Result<Element, IqError>> {
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().insert(id.to_string(), sender);
        receiver
    }

    /// Deliver a reply. Returns false when nobody was waiting for it.
    pub fn resolve(&self, id: &str, outcome: std::result::Result<Element, IqError>) -> bool {
        self.pending
            .lock()
            .remove(id)
            .is_some_and(|sender| sender.send(outcome).is_ok())
    }

    /// Drop a pending request without answering it.
    pub fn cancel(&self, id: &str) {
        self.pending.lock().remove(id);
    }

    /// Fail every request in flight, e.g. after a disconnect.
    pub fn fail_all(&self, error: &IqError) {
        let pending: Vec<_> = self.pending.lock().drain().map(|(_, tx)| tx).collect();
        for sender in pending {
            let _ = sender.send(Err(error.clone()));
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending.lock().len()
    }

    /// Send `iq` and wait for its reply, enforcing the router's timeout.
    ///
    /// # Errors
    ///
    /// Returns a timeout, protocol error or send failure when no usable reply
    /// arrives.
    pub async fn request(
        &self,
        send: impl FnOnce(&Element) -> Result<()>,
        iq: Element,
    ) -> Result<Element> {
        let id = iq
            .attr("id")
            .map_or_else(|| uuid::Uuid::new_v4().to_string(), str::to_string);
        let mut iq = iq;
        if iq.attr("id").is_none() {
            iq.set_attr("id", id.clone());
        }

        let receiver = self.register(&id);
        if let Err(error) = send(&iq) {
            self.cancel(&id);
            return Err(error);
        }

        match tokio::time::timeout(self.timeout, receiver).await {
            Ok(Ok(Ok(reply))) => Ok(reply),
            Ok(Ok(Err(error))) => Err(XmppError::ProtocolError(error.to_string())),
            Ok(Err(_)) => Err(XmppError::ProtocolError(
                "IQ request was cancelled".to_string(),
            )),
            Err(_) => {
                self.cancel(&id);
                Err(XmppError::Timeout(format!(
                    "no reply to IQ {id} within {:?}",
                    self.timeout
                )))
            }
        }
    }
}

/// Extract the typed error from an `<iq type="error">` element.
#[must_use]
pub fn parse_iq_error(elem: &Element) -> IqError {
    let error = elem.get_child("error", minidom::NSChoice::Any);
    let kind = error
        .and_then(|error| error.attr("type"))
        .unwrap_or("cancel")
        .to_string();
    let condition = error.and_then(|error| {
        error
            .children()
            .find(|child| child.name() != "text")
            .map(|child| child.name().to_string())
    });
    let text = error
        .and_then(|error| error.get_child("text", minidom::NSChoice::Any))
        .map(minidom::Element::text)
        .filter(|text| !text.trim().is_empty());

    IqError::Remote {
        kind,
        condition,
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn reply(id: &str) -> Element {
        Element::builder("iq", "jabber:client")
            .attr("type", "result")
            .attr("id", id)
            .build()
    }

    #[tokio::test]
    async fn routes_a_result_to_its_waiter() {
        let router = IqRouter::new(Duration::from_secs(1));
        let request = Element::builder("iq", "jabber:client")
            .attr("type", "get")
            .attr("id", "req-1")
            .build();

        let sent = std::sync::Arc::new(parking_lot::Mutex::new(None));
        let sent_for_send = sent.clone();
        let router = std::sync::Arc::new(router);
        let router_for_send = router.clone();

        let task = tokio::spawn(async move {
            router_for_send
                .request(
                    move |iq| {
                        *sent_for_send.lock() = Some(iq.clone());
                        Ok(())
                    },
                    request,
                )
                .await
        });

        tokio::task::yield_now().await;
        assert_eq!(router.pending_count(), 1);
        assert!(router.resolve("req-1", Ok(reply("req-1"))));

        let response = task.await.unwrap().unwrap();
        assert_eq!(response.name(), "iq");
        assert_eq!(sent.lock().as_ref().unwrap().attr("id"), Some("req-1"));
    }

    #[tokio::test]
    async fn times_out_and_drops_the_pending_request() {
        let router = std::sync::Arc::new(IqRouter::new(Duration::from_millis(20)));
        let request = Element::builder("iq", "jabber:client")
            .attr("type", "get")
            .attr("id", "req-2")
            .build();

        let error = router.request(|_| Ok(()), request).await.unwrap_err();
        assert!(matches!(error, XmppError::Timeout(_)));
        assert_eq!(router.pending_count(), 0);
    }

    #[test]
    fn parses_remote_errors() {
        let xml = r#"<iq xmlns="jabber:client" type="error" id="e1"><error type="auth"><not-authorized xmlns="urn:ietf:params:xml:ns:xmpp-stanzas"/><text xmlns="urn:ietf:params:xml:ns:xmpp-stanzas">nope</text></error></iq>"#;
        let elem = Element::from_str(xml).unwrap();
        let error = parse_iq_error(&elem);
        assert_eq!(
            error,
            IqError::Remote {
                kind: "auth".to_string(),
                condition: Some("not-authorized".to_string()),
                text: Some("nope".to_string()),
            }
        );
    }

    #[test]
    fn fails_everything_in_flight() {
        let router = IqRouter::default();
        let first = router.register("a");
        let second = router.register("b");
        router.fail_all(&IqError::Disconnected);

        let mut first = first;
        let mut second = second;
        assert_eq!(first.try_recv().unwrap(), Err(IqError::Disconnected));
        assert_eq!(second.try_recv().unwrap(), Err(IqError::Disconnected));
        assert_eq!(router.pending_count(), 0);
    }
}
