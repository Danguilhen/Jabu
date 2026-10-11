//! Connection management for XMPP using tokio-xmpp

use futures::{FutureExt, Sink, SinkExt, Stream, StreamExt};
use jid::Jid;
use minidom::{Element, Error as MinidomError};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_xmpp::{starttls::ServerConfig, Packet, SimpleClient};
use tracing::{debug, error, info, warn};
use xmpp_parsers::ns;

use crate::{ProxyConfig, ProxyType, Result, StreamManagement, TlsConfig, XmppError};

/// XMPP Connection wrapper around tokio-xmpp SimpleClient
pub struct Connection {
    // We use a channel to send stanzas to the event loop
    stanza_tx: RwLock<mpsc::UnboundedSender<Packet>>,
    config: RwLock<ConnectionConfig>,
    state: Arc<RwLock<ConnectionState>>,
    event_tx: mpsc::UnboundedSender<ConnectionEvent>,
    event_rx: RwLock<Option<mpsc::UnboundedReceiver<ConnectionEvent>>>,
    reconnect_attempts: Arc<RwLock<u32>>,
    keepalive_handle: RwLock<Option<JoinHandle<()>>>,
    jid: Arc<RwLock<Option<Jid>>>,
    sm: RwLock<Option<Arc<StreamManagement>>>,
    manual_disconnect: Arc<AtomicBool>,
}

impl Connection {
    pub fn new(config: ConnectionConfig) -> (Self, mpsc::UnboundedReceiver<ConnectionEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (stanza_tx, _stanza_rx) = mpsc::unbounded_channel();
        let state = Arc::new(RwLock::new(ConnectionState::Disconnected));

        let conn = Self {
            stanza_tx: RwLock::new(stanza_tx),
            config: RwLock::new(config),
            state,
            event_tx,
            event_rx: RwLock::new(Some(event_rx)),
            reconnect_attempts: Arc::new(RwLock::new(0)),
            keepalive_handle: RwLock::new(None),
            jid: Arc::new(RwLock::new(None)),
            sm: RwLock::new(None),
            manual_disconnect: Arc::new(AtomicBool::new(false)),
        };

        let event_rx = conn.event_rx.write().take().unwrap();
        (conn, event_rx)
    }

    pub fn state(&self) -> Arc<RwLock<ConnectionState>> {
        self.state.clone()
    }

    pub fn current_state(&self) -> ConnectionState {
        self.state.read().clone()
    }

    pub fn config(&self) -> ConnectionConfig {
        self.config.read().clone()
    }

    pub fn set_config(&self, config: ConnectionConfig) {
        *self.config.write() = config;
    }

    pub fn jid(&self) -> Option<Jid> {
        self.jid.read().clone()
    }

    /// Attach stream-management counters so outbound stanzas are counted.
    pub fn set_stream_management(&self, sm: Arc<StreamManagement>) {
        *self.sm.write() = Some(sm);
    }

    /// Connect to the XMPP server
    pub async fn connect(&self) -> Result<()> {
        let (
            jid,
            password,
            auto_reconnect,
            reconnect_interval,
            max_reconnect_attempts,
            keepalive_interval,
        ) = {
            let config = self.config.read();
            (
                config
                    .jid
                    .parse::<Jid>()
                    .map_err(|e| XmppError::InvalidJid(e.to_string()))?,
                config.password.clone(),
                config.auto_reconnect,
                config.reconnect_interval,
                config.max_reconnect_attempts,
                config.keepalive_interval,
            )
        };

        *self.jid.write() = Some(jid.clone());
        self.manual_disconnect.store(false, Ordering::Relaxed);

        self.set_state(ConnectionState::Connecting).await;

        let connector = self.build_connector(&jid);

        let client = SimpleClient::<ServerConfig>::new_with_jid_connector(
            connector,
            jid.clone(),
            password.unwrap_or_default(),
        )
        .await
        .map_err(|e| XmppError::ConnectionFailed(e.to_string()))?;

        let (stanza_tx, stanza_rx) = mpsc::unbounded_channel();
        *self.stanza_tx.write() = stanza_tx;

        *self.reconnect_attempts.write() = 0;

        self.set_state(ConnectionState::Connected).await;
        self.start_event_loop(client, stanza_rx).await;
        self.start_keepalive().await;

        self.emit_event(ConnectionEvent::Connected {
            jid: jid.to_string(),
        })
        .await;

        info!("Connected to XMPP server as {}", jid);
        Ok(())
    }

    fn build_connector(&self, _jid: &Jid) -> ServerConfig {
        // Use DNS SRV lookup (`_xmpp-client._tcp`) and opportunistically upgrade
        // to TLS via STARTTLS, validated against the bundled webpki roots.
        ServerConfig::UseSrv
    }

    async fn start_event_loop(
        &self,
        mut client: SimpleClient<ServerConfig>,
        mut stanza_rx: mpsc::UnboundedReceiver<Packet>,
    ) {
        let event_tx = self.event_tx.clone();
        let state = self.state.clone();
        let reconnect_attempts = self.reconnect_attempts.clone();
        let config = self.config.read().clone();
        let jid = self.jid.clone();
        let manual_disconnect = self.manual_disconnect.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = client.next() => {
                        match event {
                            Some(Ok(stanza)) => {
                                // Any incoming stanza means we're connected/authenticated
                                *state.write() = ConnectionState::Authenticated;
                                let _ = event_tx.send(ConnectionEvent::StanzaReceived(Self::element_to_string(&stanza)));
                                *jid.write() = Some(client.bound_jid().clone());
                            }
                            Some(Err(err)) => {
                                error!("XMPP stream error: {}", err);
                                *state.write() = ConnectionState::Disconnected;
                                let _ = event_tx.send(ConnectionEvent::Error(err.to_string()));

                                if config.auto_reconnect
                                    && !manual_disconnect.load(Ordering::Relaxed)
                                {
                                    Self::schedule_reconnect(
                                        reconnect_attempts.clone(),
                                        config.clone(),
                                        jid.clone(),
                                        event_tx.clone(),
                                        state.clone(),
                                    ).await;
                                }
                                break;
                            }
                            None => {
                                // Stream ended
                                break;
                            }
                        }
                    }
                    packet = stanza_rx.recv() => {
                        if let Some(packet) = packet {
                            if let Err(e) = client.send(packet).await {
                                error!("Failed to send packet: {}", e);
                                let _ = event_tx.send(ConnectionEvent::Error(e.to_string()));
                                break;
                            }
                        } else {
                            // Channel closed
                            break;
                        }
                    }
                }
            }

            // Stream ended - handle disconnect
            *state.write() = ConnectionState::Disconnected;
            let _ = event_tx.send(ConnectionEvent::Disconnected {
                reason: "Stream ended".to_string(),
            });

            if config.auto_reconnect && !manual_disconnect.load(Ordering::Relaxed) {
                Self::schedule_reconnect(
                    reconnect_attempts.clone(),
                    config.clone(),
                    jid.clone(),
                    event_tx.clone(),
                    state.clone(),
                )
                .await;
            }
        });
    }

    async fn schedule_reconnect(
        reconnect_attempts: Arc<RwLock<u32>>,
        config: ConnectionConfig,
        jid: Arc<RwLock<Option<Jid>>>,
        event_tx: mpsc::UnboundedSender<ConnectionEvent>,
        state: Arc<RwLock<ConnectionState>>,
    ) {
        let attempts = *reconnect_attempts.read();
        if attempts >= config.max_reconnect_attempts {
            error!("Max reconnection attempts reached");
            let _ = event_tx.send(ConnectionEvent::ReconnectFailed);
            return;
        }

        *reconnect_attempts.write() = attempts + 1;

        let delay = Duration::from_secs(config.reconnect_interval * 2u64.pow(attempts));
        info!(
            "Reconnecting in {:?} (attempt {}/{})",
            delay,
            attempts + 1,
            config.max_reconnect_attempts
        );

        *state.write() = ConnectionState::Reconnecting {
            attempt: attempts + 1,
        };
        let _ = event_tx.send(ConnectionEvent::Reconnecting {
            attempt: attempts + 1,
        });

        tokio::time::sleep(delay).await;

        // Trigger reconnect - emit event to let upper layer handle it
        let _ = event_tx.send(ConnectionEvent::ReconnectTriggered);
    }

    async fn start_keepalive(&self) {
        let config = self.config.read();
        if config.keepalive_interval == 0 {
            return;
        }
        let keepalive_interval = config.keepalive_interval;
        drop(config);

        let stanza_tx = self.stanza_tx.read().clone();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(keepalive_interval));
            loop {
                interval.tick().await;

                // Send a whitespace keepalive (XEP-0199)
                let mut ping = Element::builder("iq", ns::JABBER_CLIENT)
                    .attr("type", "get")
                    .attr("id", "keepalive")
                    .build();
                ping.append_child(Element::builder("ping", ns::PING).build());

                if let Err(e) = stanza_tx.send(Packet::Stanza(ping)) {
                    warn!("Keepalive ping failed: {}", e);
                    break;
                }
            }
        });

        *self.keepalive_handle.write() = Some(handle);
    }

    /// Convert minidom Element to String for serialization
    fn element_to_string(element: &Element) -> String {
        let mut buf = Vec::new();
        element.write_to(&mut buf).unwrap_or_default();
        String::from_utf8(buf).unwrap_or_default()
    }

    /// Disconnect from the XMPP server
    pub async fn disconnect(&self) -> Result<()> {
        self.manual_disconnect.store(true, Ordering::Relaxed);
        if let Some(handle) = self.keepalive_handle.write().take() {
            handle.abort();
        }

        // Close the stanza channel by dropping the sender
        let stanza_tx = self.stanza_tx.read().clone();
        drop(stanza_tx);

        self.set_state(ConnectionState::Disconnected).await;
        self.emit_event(ConnectionEvent::Disconnected {
            reason: "User requested".to_string(),
        })
        .await;

        info!("Disconnected from XMPP server");
        Ok(())
    }

    /// Send a raw stanza
    pub async fn send_stanza(&self, stanza: Element) -> Result<()> {
        self.send_stanza_now(stanza)
    }

    /// Send a raw stanza without awaiting (the underlying channel is unbounded)
    pub fn send_stanza_now(&self, stanza: Element) -> Result<()> {
        let is_stanza = matches!(stanza.name(), "message" | "presence" | "iq");
        self.stanza_tx
            .read()
            .send(Packet::Stanza(stanza))
            .map_err(|e| XmppError::SendFailed(e.to_string()))?;

        if is_stanza {
            if let Some(sm) = self.sm.read().as_ref() {
                sm.note_sent();
            }
        }

        Ok(())
    }

    async fn set_state(&self, state: ConnectionState) {
        *self.state.write() = state;
    }

    async fn emit_event(&self, event: ConnectionEvent) {
        let _ = self.event_tx.send(event);
    }
}

/// Connection state machine
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Authenticated,
    Reconnecting { attempt: u32 },
    Failed { reason: String },
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl ConnectionState {
    pub fn is_connected(&self) -> bool {
        matches!(
            self,
            ConnectionState::Connected | ConnectionState::Authenticated
        )
    }

    pub fn is_connecting(&self) -> bool {
        matches!(
            self,
            ConnectionState::Connecting | ConnectionState::Reconnecting { .. }
        )
    }
}

/// Connection events emitted to the event bus
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ConnectionEvent {
    Connected { jid: String },
    Authenticated,
    Disconnected { reason: String },
    Reconnecting { attempt: u32 },
    ReconnectTriggered,
    ReconnectFailed,
    StanzaReceived(String),
    Error(String),
}

/// Connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub jid: String,
    pub password: Option<String>,
    pub resource: Option<String>,
    pub priority: i32,
    pub auto_reconnect: bool,
    pub reconnect_interval: u64,
    pub max_reconnect_attempts: u32,
    pub enable_stream_management: bool,
    pub enable_compression: bool,
    pub keepalive_interval: u64,
    pub tls_config: TlsConfig,
    pub proxy_config: Option<ProxyConfig>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            jid: String::new(),
            password: None,
            resource: Some("jabu".to_string()),
            priority: 0,
            auto_reconnect: true,
            reconnect_interval: 5,
            max_reconnect_attempts: 10,
            enable_stream_management: true,
            enable_compression: false,
            keepalive_interval: 60,
            tls_config: TlsConfig::default(),
            proxy_config: None,
        }
    }
}
