//! Connection management for XMPP using tokio-xmpp

use futures::{SinkExt, StreamExt};
use jid::Jid;
use minidom::Element;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_xmpp::{starttls::ServerConfig, Packet};
use tracing::{debug, error, info, warn};
use xmpp_parsers::ns;

use crate::{ProxyConfig, Result, StreamManagement, TlsConfig, XmppError};

mod connector;
mod login;
mod register;

use connector::{ClientStream, Connector, CustomConnector};

/// XMPP connection wrapper around a tokio-xmpp stream
pub struct Connection {
    // We use a channel to send stanzas to the event loop
    stanza_tx: RwLock<mpsc::UnboundedSender<Packet>>,
    config: RwLock<ConnectionConfig>,
    state: Arc<RwLock<ConnectionState>>,
    event_tx: mpsc::UnboundedSender<ConnectionEvent>,
    event_rx: RwLock<Option<mpsc::UnboundedReceiver<ConnectionEvent>>>,
    reconnect_attempts: Arc<RwLock<u32>>,
    event_loop_handle: RwLock<Option<JoinHandle<()>>>,
    keepalive_handle: RwLock<Option<JoinHandle<()>>>,
    jid: Arc<RwLock<Option<Jid>>>,
    sm: RwLock<Option<Arc<StreamManagement>>>,
    manual_disconnect: Arc<AtomicBool>,
    last_inbound: Arc<parking_lot::Mutex<Instant>>,
    ping_timeout: Arc<tokio::sync::Notify>,
    csi_active: Arc<AtomicBool>,
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
            event_loop_handle: RwLock::new(None),
            keepalive_handle: RwLock::new(None),
            jid: Arc::new(RwLock::new(None)),
            sm: RwLock::new(None),
            manual_disconnect: Arc::new(AtomicBool::new(false)),
            last_inbound: Arc::new(parking_lot::Mutex::new(Instant::now())),
            ping_timeout: Arc::new(tokio::sync::Notify::new()),
            csi_active: Arc::new(AtomicBool::new(true)),
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

    /// Whether the client currently wants to receive all traffic (XEP-0352).
    pub fn csi_active(&self) -> bool {
        self.csi_active.load(Ordering::Relaxed)
    }

    /// Remember the client-state preference without sending anything yet.
    pub fn set_csi_active(&self, active: bool) {
        self.csi_active.store(active, Ordering::Relaxed);
    }

    /// Tell the server the current client-state preference (XEP-0352).
    pub fn send_csi_state(&self) -> Result<()> {
        if !self.current_state().is_connected() {
            return Ok(());
        }
        let name = if self.csi_active() {
            "active"
        } else {
            "inactive"
        };
        self.send_stanza_now(Element::builder(name, "urn:xmpp:csi:0").build())
    }

    /// Create an account in-band (XEP-0077) using the configured transport.
    ///
    /// # Errors
    ///
    /// Returns an error when the server rejects the request or the stream
    /// cannot be opened.
    pub async fn register_account(&self, username: &str, password: &str) -> Result<()> {
        let domain = self
            .config
            .read()
            .jid
            .parse::<Jid>()
            .map(|jid| jid.domain().to_string())
            .or_else(|_| {
                self.jid
                    .read()
                    .as_ref()
                    .map(|jid| jid.domain().to_string())
                    .ok_or_else(|| XmppError::InvalidJid("no server domain configured".into()))
            })?;
        let jid: Jid = format!("{username}@{domain}")
            .parse()
            .map_err(|error: jid::Error| XmppError::InvalidJid(error.to_string()))?;
        let connector = self.build_connector(&jid)?;
        register::register(&connector, &domain, username, password).await
    }

    /// Pick a single rustls crypto provider before any TLS config is built.
    ///
    /// The dependency graph can enable both `ring` and `aws-lc-rs`; rustls then
    /// refuses to choose one implicitly.
    fn install_crypto_provider() {
        static INSTALL: std::sync::Once = std::sync::Once::new();
        INSTALL.call_once(|| {
            if tokio_rustls::rustls::crypto::ring::default_provider()
                .install_default()
                .is_err()
            {
                debug!("rustls crypto provider was already installed");
            }
        });
    }

    /// Attach stream-management counters so outbound stanzas are counted.
    pub fn set_stream_management(&self, sm: Arc<StreamManagement>) {
        *self.sm.write() = Some(sm);
    }

    /// Connect to the XMPP server
    pub async fn connect(&self) -> Result<()> {
        let (jid, password) = {
            let config = self.config.read();
            let base = config
                .jid
                .split('/')
                .next()
                .unwrap_or(&config.jid)
                .parse::<Jid>()
                .map_err(|e| XmppError::InvalidJid(e.to_string()))?;
            let jid = match config.resource.as_deref().filter(|r| !r.trim().is_empty()) {
                Some(resource) => base
                    .to_bare()
                    .with_resource_str(resource)
                    .map(Jid::from)
                    .map_err(|e| XmppError::InvalidJid(e.to_string()))?,
                None => base,
            };
            (jid, config.password.clone())
        };

        if let Some(handle) = self.event_loop_handle.write().take() {
            handle.abort();
        }
        *self.jid.write() = Some(jid.clone());
        self.manual_disconnect.store(false, Ordering::Relaxed);

        self.set_state(ConnectionState::Connecting).await;
        Self::install_crypto_provider();

        let connector = self.build_connector(&jid)?;
        *self.last_inbound.lock() = Instant::now();

        let sm = self.sm.read().clone();
        let login = login::login(connector, jid.clone(), password.unwrap_or_default(), sm)
            .await
            .map_err(|e| XmppError::ConnectionFailed(e.to_string()))?;
        let client = login.stream;
        let bound_jid = client.jid.clone();
        *self.jid.write() = Some(bound_jid.clone());

        let (stanza_tx, stanza_rx) = mpsc::unbounded_channel();
        *self.stanza_tx.write() = stanza_tx;

        *self.reconnect_attempts.write() = 0;

        self.set_state(ConnectionState::Connected).await;
        self.start_event_loop(client, stanza_rx).await;
        self.start_keepalive().await;

        self.emit_event(ConnectionEvent::Connected {
            jid: bound_jid.to_string(),
        })
        .await;

        info!(
            "Connected to XMPP server as {} (resumed: {})",
            bound_jid, login.resumed
        );
        Ok(())
    }

    fn build_connector(&self, jid: &Jid) -> Result<Connector> {
        let tls = self.config.read().tls_config.clone();
        let default_tls = tls.verify_cert
            && tls.verify_hostname
            && tls.ca_file.is_none()
            && tls.cert_file.is_none()
            && tls.key_file.is_none();

        let proxy = self.config.read().proxy_config.clone();
        let direct_tls = self.config.read().direct_tls;

        if default_tls && proxy.is_none() && !direct_tls {
            // DNS SRV lookup (`_xmpp-client._tcp`) with the bundled webpki roots.
            Ok(Connector::System(ServerConfig::UseSrv))
        } else {
            let host = jid.domain().to_string();
            let port = if direct_tls { 5223 } else { 5222 };
            CustomConnector::new(host, port, &tls, proxy, direct_tls)
                .map(Connector::Custom)
                .map_err(|e| XmppError::TlsError(e.to_string()))
        }
    }

    async fn start_event_loop(
        &self,
        mut client: ClientStream,
        mut stanza_rx: mpsc::UnboundedReceiver<Packet>,
    ) {
        let event_tx = self.event_tx.clone();
        let state = self.state.clone();
        let reconnect_attempts = self.reconnect_attempts.clone();
        let config = self.config.read().clone();
        let jid = self.jid.clone();
        let manual_disconnect = self.manual_disconnect.clone();
        let last_inbound = self.last_inbound.clone();
        let ping_timeout = self.ping_timeout.clone();
        let sm = self.sm.read().clone();

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = client.next() => {
                        match event {
                            Some(Ok(Packet::Stanza(stanza))) => {
                                // Any incoming stanza means we're connected/authenticated
                                *state.write() = ConnectionState::Authenticated;
                                *last_inbound.lock() = Instant::now();
                                let _ = event_tx.send(ConnectionEvent::StanzaReceived(Self::element_to_string(&stanza)));
                                *jid.write() = Some(client.jid.clone());
                            }
                            Some(Ok(Packet::Text(_))) => {}
                            Some(Ok(Packet::StreamStart(_))) | Some(Ok(Packet::StreamEnd)) => {
                                break;
                            }
                            Some(Err(err)) => {
                                error!("XMPP stream error: {}", err);
                                *state.write() = ConnectionState::Disconnected;
                                let _ = event_tx.send(ConnectionEvent::Error(err.to_string()));
                                break;
                            }
                            None => {
                                break;
                            }
                        }
                    }
                    packet = stanza_rx.recv() => {
                        if let Some(packet) = packet {
                            if let (Some(sm), Packet::Stanza(stanza)) = (sm.as_deref(), &packet) {
                                if sm.is_enabled() {
                                    sm.note_sent_stanza(stanza.clone());
                                }
                            }
                            if let Err(e) = client.send(packet).await {
                                error!("Failed to send packet: {}", e);
                                let _ = event_tx.send(ConnectionEvent::Error(e.to_string()));
                                break;
                            }
                            if let Some(sm) = sm.as_deref() {
                                if sm.is_enabled() && sm.unacked() >= 5 {
                                    let request = Element::builder("r", "urn:xmpp:sm:3").build();
                                    if let Err(e) = client.send(Packet::Stanza(request)).await {
                                        warn!("Failed to request a stream-management ack: {}", e);
                                    }
                                }
                            }
                        } else {
                            // Channel closed
                            break;
                        }
                    }
                    _ = ping_timeout.notified() => {
                        error!("XEP-0199 keepalive timed out; dropping the connection");
                        let _ = event_tx.send(ConnectionEvent::Error(
                            "XEP-0199 keepalive timed out".to_string(),
                        ));
                        break;
                    }
                }
            }

            // Stream ended - handle disconnect, then reconnect unless the user asked for it.
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
        *self.event_loop_handle.write() = Some(handle);
    }

    async fn schedule_reconnect(
        reconnect_attempts: Arc<RwLock<u32>>,
        config: ConnectionConfig,
        _jid: Arc<RwLock<Option<Jid>>>,
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

        if let Some(handle) = self.keepalive_handle.write().take() {
            handle.abort();
        }
        let stanza_tx = self.stanza_tx.read().clone();
        let last_inbound = self.last_inbound.clone();
        let ping_timeout = self.ping_timeout.clone();
        let handle = tokio::spawn(async move {
            let period = Duration::from_secs(keepalive_interval);
            let mut interval = tokio::time::interval(period);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await;
            let mut awaiting_pong: Option<Instant> = None;

            loop {
                interval.tick().await;

                if let Some(sent_at) = awaiting_pong {
                    let idle = last_inbound.lock().elapsed();
                    if idle >= period && sent_at.elapsed() >= period {
                        warn!(
                            "No keepalive response for {:?}; declaring the connection dead",
                            sent_at.elapsed()
                        );
                        ping_timeout.notify_waiters();
                        break;
                    }
                }

                let mut ping = Element::builder("iq", ns::JABBER_CLIENT)
                    .attr("type", "get")
                    .attr("id", format!("keepalive-{}", uuid::Uuid::new_v4()))
                    .build();
                ping.append_child(Element::builder("ping", ns::PING).build());

                if let Err(e) = stanza_tx.send(Packet::Stanza(ping)) {
                    warn!("Keepalive ping failed: {}", e);
                    break;
                }
                awaiting_pong = Some(Instant::now());
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
        if let Some(sm) = self.sm.read().clone() {
            sm.clear_resumption();
        }
        if let Some(handle) = self.keepalive_handle.write().take() {
            handle.abort();
        }
        if let Some(handle) = self.event_loop_handle.write().take() {
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

    /// Parse and send an XML stanza string.
    pub fn send_stanza_str(&self, xml: &str) -> Result<()> {
        let stanza =
            Element::from_str(xml).map_err(|e| XmppError::ParseError(format!("{}: {}", e, xml)))?;
        self.send_stanza_now(stanza)
    }

    /// Send a raw stanza without awaiting (the underlying channel is unbounded)
    pub fn send_stanza_now(&self, stanza: Element) -> Result<()> {
        self.stanza_tx
            .read()
            .send(Packet::Stanza(stanza))
            .map_err(|e| XmppError::SendFailed(e.to_string()))
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
    pub direct_tls: bool,
    pub presence_show: Option<String>,
    pub presence_status: Option<String>,
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
            direct_tls: false,
            presence_show: None,
            presence_status: None,
        }
    }
}
