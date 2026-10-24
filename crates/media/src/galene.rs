//! Galène SFU client for Discord-style voice channels.
//!
//! Each voice channel maps to a Galène group. The client publishes one
//! microphone stream and receives one stream per remote participant, so the
//! server (not the client) does the fan-out and a channel can hold well over
//! ten participants.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use async_tungstenite::tungstenite::Message;
use futures::{SinkExt, StreamExt};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_webrtc as gst_webrtc;
use parking_lot::RwLock;
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;
use xmpp_core::voice::{VoiceChannel, VoiceEvent, VoiceJoin, VoiceParticipant, VoicePort};

use crate::gstreamer::{
    apply_mute, audio_devices_with_id, create_answer, create_offer, set_remote_description,
};

const ICE_SERVERS_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Default)]
struct IceServer {
    urls: Vec<String>,
    username: Option<String>,
    credential: Option<String>,
}

/// A Galène-backed voice backend.
pub struct GaleneVoicePort {
    session: RwLock<Option<Arc<Session>>>,
    events: broadcast::Sender<VoiceEvent>,
    server: String,
    group: String,
    password: Option<String>,
}

impl GaleneVoicePort {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure the Galène endpoint. This backend is kept for the library
    /// and tests only; shipped space voice channels use the XMPP conference.
    #[must_use]
    pub fn with_sfu(
        server: impl Into<String>,
        group: impl Into<String>,
        password: Option<String>,
    ) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            session: RwLock::new(None),
            events,
            server: server.into(),
            group: group.into(),
            password,
        }
    }
}

impl Default for GaleneVoicePort {
    fn default() -> Self {
        Self::with_sfu(String::new(), String::new(), None)
    }
}

impl GaleneVoicePort {
    /// Number of open media streams and answered incoming streams.
    ///
    /// Used by the loopback integration test; not part of the stable API.
    #[doc(hidden)]
    #[must_use]
    pub fn stream_stats(&self) -> (usize, usize) {
        self.session
            .read()
            .as_ref()
            .map(|session| {
                (
                    session.backends.read().len(),
                    session.answered.load(std::sync::atomic::Ordering::Relaxed),
                )
            })
            .unwrap_or((0, 0))
    }
}

#[async_trait]
impl VoicePort for GaleneVoicePort {
    async fn join(&self, request: VoiceJoin) -> Result<()> {
        let existing = { self.session.read().clone() };
        if let Some(session) = existing {
            let _ = session.leave().await;
        }
        let session = Session::connect(
            request,
            &self.server,
            &self.group,
            self.password.clone(),
            self.events.clone(),
        )
        .await?;
        *self.session.write() = Some(session);
        Ok(())
    }

    async fn leave(&self) -> Result<()> {
        let session = self.session.write().take();
        if let Some(session) = session {
            session.leave().await?;
        }
        Ok(())
    }

    async fn set_muted(&self, muted: bool) -> Result<()> {
        let session = self.session.read().clone();
        if let Some(session) = session {
            session.set_muted(muted);
        }
        Ok(())
    }

    async fn set_participant_volume(&self, id: &str, volume: f64) -> Result<()> {
        let session = self.session.read().clone();
        if let Some(session) = session {
            session.set_participant_volume(id, volume);
        }
        Ok(())
    }

    fn is_joined(&self) -> bool {
        self.session
            .read()
            .as_ref()
            .is_some_and(|session| session.joined.load(Ordering::Relaxed))
    }

    fn channel(&self) -> Option<VoiceChannel> {
        self.session
            .read()
            .as_ref()
            .map(|session| session.channel.clone())
    }

    fn participants(&self) -> Vec<VoiceParticipant> {
        self.session
            .read()
            .as_ref()
            .map(|session| session.participants())
            .unwrap_or_default()
    }

    fn subscribe(&self) -> broadcast::Receiver<VoiceEvent> {
        self.events.subscribe()
    }
}

type WsSink =
    Box<dyn futures::Sink<Message, Error = async_tungstenite::tungstenite::Error> + Send + Unpin>;
type WsStream = Box<
    dyn futures::Stream<Item = Result<Message, async_tungstenite::tungstenite::Error>>
        + Send
        + Unpin,
>;

struct Session {
    channel: VoiceChannel,
    group: String,
    id: String,
    username: String,
    microphone: Option<String>,
    joined: AtomicBool,
    muted: AtomicBool,
    participants: RwLock<Vec<VoiceParticipant>>,
    out: mpsc::UnboundedSender<Value>,
    backends: RwLock<HashMap<String, StreamBackend>>,
    up: RwLock<Option<String>>,
    answered: std::sync::atomic::AtomicUsize,
    ice_servers: RwLock<Vec<IceServer>>,
    close_requested: AtomicBool,
    last_voice: RwLock<HashMap<String, std::time::Instant>>,
    volumes: RwLock<HashMap<String, f64>>,
    events: broadcast::Sender<VoiceEvent>,
}

impl Session {
    async fn connect(
        request: VoiceJoin,
        server: &str,
        group: &str,
        password: Option<String>,
        events: broadcast::Sender<VoiceEvent>,
    ) -> Result<Arc<Self>> {
        let base = server.trim_end_matches('/').to_string();
        if base.is_empty() {
            return Err(anyhow!("no Galène server configured"));
        }
        let group = group.trim_matches('/').to_string();
        let status_url = format!("{base}/{group}/.status");
        let status = tokio::time::timeout(
            ICE_SERVERS_TIMEOUT,
            reqwest::Client::new().get(&status_url).send(),
        )
        .await
        .map_err(|_| anyhow!("timed out fetching {status_url}"))?
        .context("could not reach the Galène server")?
        .error_for_status()
        .context("Galène rejected the group status request")?
        .json::<Value>()
        .await
        .context("invalid Galène group status")?;

        let endpoint = status
            .get("endpoint")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| default_endpoint(&base));

        let (ws, _) = tokio::time::timeout(
            ICE_SERVERS_TIMEOUT,
            async_tungstenite::tokio::connect_async(endpoint.as_str()),
        )
        .await
        .map_err(|_| anyhow!("timed out connecting to {endpoint}"))?
        .context("could not open the Galène websocket")?;

        let id = Uuid::new_v4().simple().to_string();
        let username = if request.username.trim().is_empty() {
            "jabu".to_string()
        } else {
            request.username.trim().to_string()
        };
        let (out, out_rx) = mpsc::unbounded_channel();
        let (joined_tx, joined_rx) = oneshot::channel();
        let session = Arc::new(Self {
            channel: request.channel,
            group: group.clone(),
            id: id.clone(),
            username: username.clone(),
            microphone: request.microphone.clone(),
            joined: AtomicBool::new(false),
            muted: AtomicBool::new(request.muted),
            participants: RwLock::new(Vec::new()),
            out,
            backends: RwLock::new(HashMap::new()),
            up: RwLock::new(None),
            answered: std::sync::atomic::AtomicUsize::new(0),
            ice_servers: RwLock::new(Vec::new()),
            close_requested: AtomicBool::new(false),
            last_voice: RwLock::new(HashMap::new()),
            volumes: RwLock::new(HashMap::new()),
            events: events.clone(),
        });

        let (sink, stream) = ws.split();
        let sink: WsSink = Box::new(sink);
        let stream: WsStream = Box::new(stream);
        let runner = session.clone();
        tokio::spawn(async move {
            runner.run(sink, stream, out_rx, joined_tx, password).await;
        });

        match tokio::time::timeout(Duration::from_secs(15), joined_rx).await {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(error))) => return Err(anyhow!(error)),
            Ok(Err(_)) => return Err(anyhow!("Galène connection closed while joining")),
            Err(_) => return Err(anyhow!("timed out joining the Galène group")),
        }

        if let Err(error) = session.start_publishing().await {
            let _ = session.leave().await;
            return Err(error);
        }
        Ok(session)
    }

    async fn run(
        self: Arc<Self>,
        mut sink: WsSink,
        mut stream: WsStream,
        mut out_rx: mpsc::UnboundedReceiver<Value>,
        joined_tx: oneshot::Sender<std::result::Result<(), String>>,
        password: Option<String>,
    ) {
        let mut joined_tx = Some(joined_tx);
        self.send(json!({
            "type": "handshake",
            "version": ["2", "1"],
            "id": self.id,
        }));
        self.send(json!({
            "type": "join",
            "kind": "join",
            "group": self.group,
            "username": self.username,
            "password": password.unwrap_or_default(),
        }));

        let mut ping = tokio::time::interval(Duration::from_secs(20));
        ping.tick().await;

        loop {
            tokio::select! {
                _ = ping.tick() => {
                    self.send(json!({ "type": "ping" }));
                }
                outbound = out_rx.recv() => {
                    let Some(value) = outbound else {
                        break;
                    };
                    if value.get("type").and_then(Value::as_str) == Some("__close") {
                        let _ = sink.send(Message::Close(None)).await;
                        break;
                    }
                    if sink
                        .send(Message::Text(value.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                incoming = stream.next() => {
                    match incoming {
                        Some(Ok(Message::Text(text))) => {
                            let Ok(value) = serde_json::from_str::<Value>(&text) else {
                                continue;
                            };
                            if let Err(error) =
                                self.handle_message(value, &mut joined_tx).await
                            {
                                tracing::warn!("Galène message error: {error}");
                                let _ = self.events.send(VoiceEvent::Error(error.to_string()));
                            }
                            if self.close_requested.load(Ordering::Relaxed) {
                                break;
                            }
                        }
                        Some(Ok(Message::Ping(payload))) => {
                            let _ = sink.send(Message::Pong(payload)).await;
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Err(error)) => {
                            let _ = self.events.send(VoiceEvent::Error(error.to_string()));
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(tx) = joined_tx.take() {
            let _ = tx.send(Err("connection closed".to_string()));
        }
        self.shutdown();
        let _ = self.events.send(VoiceEvent::Left);
    }

    async fn handle_message(
        self: &Arc<Self>,
        value: Value,
        joined_tx: &mut Option<oneshot::Sender<std::result::Result<(), String>>>,
    ) -> Result<()> {
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind {
            "handshake" => {}
            "ping" => self.send(json!({ "type": "pong" })),
            "joined" => match value
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
            {
                "join" => {
                    *self.ice_servers.write() = parse_ice_servers(&value);
                    self.joined.store(true, Ordering::Relaxed);
                    if let Some(tx) = joined_tx.take() {
                        let _ = tx.send(Ok(()));
                    }
                    let _ = self.events.send(VoiceEvent::Joined);
                    self.publish_participants();
                    self.request_audio();
                }
                "leave" | "fail" => {
                    self.close_requested.store(true, Ordering::Relaxed);
                    if let Some(tx) = joined_tx.take() {
                        let message = value
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or("could not join the voice channel")
                            .to_string();
                        let _ = tx.send(Err(message));
                    }
                    self.joined.store(false, Ordering::Relaxed);
                }
                _ => {}
            },
            "user" => {
                self.update_user(&value);
            }
            "offer" => {
                let id = value
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("stream offer without id"))?
                    .to_string();
                let sdp = value
                    .get("sdp")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("stream offer without sdp"))?;
                let label = value
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("camera")
                    .to_string();
                let source = value
                    .get("source")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                self.answer_stream(id, label, source, sdp).await?;
            }
            "answer" => {
                let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
                let sdp = value.get("sdp").and_then(Value::as_str).unwrap_or_default();
                let webrtc = self
                    .backends
                    .read()
                    .get(id)
                    .map(|backend| backend.webrtc.clone());
                if let Some(webrtc) = webrtc {
                    set_remote_description(&webrtc, sdp, gst_webrtc::WebRTCSDPType::Answer).await?;
                }
            }
            "ice" => {
                let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
                if let Some(candidate) = value.get("candidate") {
                    let candidate_sdp = candidate
                        .get("candidate")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let mline = candidate
                        .get("sdpMLineIndex")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as u32;
                    let webrtc = self
                        .backends
                        .read()
                        .get(id)
                        .map(|backend| backend.webrtc.clone());
                    if let Some(webrtc) = webrtc {
                        webrtc.emit_by_name::<()>("add-ice-candidate", &[&mline, &candidate_sdp]);
                    }
                }
            }
            "close" | "abort" => {
                let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
                if let Some(backend) = self.backends.write().remove(id) {
                    backend.stop();
                }
            }
            "error" => {
                let message = value
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or("Galène error")
                    .to_string();
                let _ = self.events.send(VoiceEvent::Error(message));
            }
            _ => {}
        }
        Ok(())
    }

    fn update_user(&self, value: &Value) {
        let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut participants = self.participants.write();
        match kind {
            "add" | "change" => {
                let name = value
                    .get("username")
                    .and_then(Value::as_str)
                    .unwrap_or("participant")
                    .to_string();
                let has_audio = value
                    .get("streams")
                    .and_then(Value::as_object)
                    .is_some_and(|streams| !streams.is_empty());
                let entry = VoiceParticipant {
                    id: id.to_string(),
                    name,
                    muted: !has_audio,
                    speaking: false,
                    screen_sharing: false,
                    is_self: id == self.id,
                    volume: 1.0,
                };
                if let Some(existing) = participants.iter_mut().find(|p| p.id == id) {
                    *existing = entry;
                } else {
                    participants.push(entry);
                }
            }
            "delete" => participants.retain(|participant| participant.id != id),
            _ => {}
        }
        if !participants.iter().any(|participant| participant.is_self) {
            participants.push(VoiceParticipant {
                id: self.id.clone(),
                name: self.username.clone(),
                muted: self.muted.load(Ordering::Relaxed),
                speaking: false,
                screen_sharing: false,
                is_self: true,
                volume: 1.0,
            });
        }
        participants.sort_by(|a, b| b.is_self.cmp(&a.is_self).then(a.name.cmp(&b.name)));
        let snapshot = participants.clone();
        drop(participants);
        let _ = self.events.send(VoiceEvent::Participants(snapshot));
    }

    fn publish_participants(&self) {
        let mut participants = self.participants.write();
        if let Some(me) = participants
            .iter_mut()
            .find(|participant| participant.is_self)
        {
            me.muted = self.muted.load(Ordering::Relaxed);
        } else {
            participants.push(VoiceParticipant {
                id: self.id.clone(),
                name: self.username.clone(),
                muted: self.muted.load(Ordering::Relaxed),
                speaking: false,
                screen_sharing: false,
                is_self: true,
                volume: 1.0,
            });
        }
        participants.sort_by(|a, b| b.is_self.cmp(&a.is_self).then(a.name.cmp(&b.name)));
        let snapshot = participants.clone();
        drop(participants);
        let _ = self.events.send(VoiceEvent::Participants(snapshot));
    }

    fn participants(&self) -> Vec<VoiceParticipant> {
        let now = std::time::Instant::now();
        let last_voice = self.last_voice.read();
        let volumes = self.volumes.read();
        let mut participants = self.participants.read().clone();
        for participant in &mut participants {
            participant.speaking = last_voice
                .get(&participant.id)
                .is_some_and(|last| now.duration_since(*last) < Duration::from_millis(500));
            participant.volume = volumes.get(&participant.id).copied().unwrap_or(1.0);
        }
        participants
    }

    fn note_voice(&self, id: &str) {
        self.last_voice
            .write()
            .insert(id.to_string(), std::time::Instant::now());
    }

    fn set_participant_volume(&self, id: &str, volume: f64) {
        let volume = volume.clamp(0.0, 2.0);
        self.volumes.write().insert(id.to_string(), volume);
        for backend in self.backends.read().values() {
            if backend.participant_id.as_deref() == Some(id) {
                if let Some(element) = backend.volume.as_ref() {
                    element.set_property("volume", volume);
                }
            }
        }
    }

    fn request_audio(&self) {
        self.send(json!({
            "type": "request",
            "request": { "": ["audio"] },
        }));
    }

    fn send(&self, value: Value) {
        let _ = self.out.send(value);
    }

    fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
        if let Some(id) = self.up.read().clone() {
            if let Some(backend) = self.backends.read().get(&id) {
                if let Some(volume) = backend.volume.as_ref() {
                    volume.set_property("mute", muted);
                }
                if let Some(source) = backend.source.as_ref() {
                    apply_mute(source, muted);
                }
            }
        }
        self.publish_participants();
    }

    async fn start_publishing(self: &Arc<Self>) -> Result<()> {
        let id = Uuid::new_v4().simple().to_string();
        let backend = build_up_stream(
            self.id.clone(),
            id.clone(),
            self.out.clone(),
            self.muted.load(Ordering::Relaxed),
            self.microphone.as_deref(),
            &self.ice_servers.read(),
        )?;
        let sdp = create_offer(&backend.webrtc).await?;
        self.backends.write().insert(id.clone(), backend);
        *self.up.write() = Some(id.clone());
        self.send(json!({
            "type": "offer",
            "source": self.id,
            "username": self.username,
            "kind": "",
            "id": id,
            "label": "camera",
            "sdp": sdp,
        }));
        Ok(())
    }

    async fn answer_stream(
        self: &Arc<Self>,
        id: String,
        _label: String,
        source: Option<String>,
        sdp: &str,
    ) -> Result<()> {
        let backend = build_down_stream(
            self.id.clone(),
            id.clone(),
            self.out.clone(),
            &self.ice_servers.read(),
            self,
            source.clone(),
        )?;
        self.backends.write().insert(id.clone(), backend);
        let webrtc = {
            let backends = self.backends.read();
            let backend = backends
                .get(&id)
                .ok_or_else(|| anyhow!("stream vanished"))?;
            backend.webrtc.clone()
        };
        set_remote_description(&webrtc, sdp, gst_webrtc::WebRTCSDPType::Offer).await?;
        let answer = create_answer(&webrtc).await?;
        self.send(json!({ "type": "answer", "id": id, "sdp": answer }));
        self.answered.fetch_add(1, Ordering::Relaxed);

        if let Some(source) = source {
            let mut participants = self.participants.write();
            if let Some(participant) = participants
                .iter_mut()
                .find(|participant| participant.id == source)
            {
                participant.muted = false;
            }
            let snapshot = participants.clone();
            drop(participants);
            let _ = self.events.send(VoiceEvent::Participants(snapshot));
        }
        Ok(())
    }

    async fn leave(&self) -> Result<()> {
        if self.joined.swap(false, Ordering::Relaxed) {
            self.send(json!({
                "type": "join",
                "kind": "leave",
                "group": self.group,
            }));
            self.send(json!({ "type": "__close" }));
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
        self.shutdown();
        let _ = self.events.send(VoiceEvent::Left);
        Ok(())
    }

    fn shutdown(&self) {
        self.joined.store(false, Ordering::Relaxed);
        let backends = std::mem::take(&mut *self.backends.write());
        for (_, backend) in backends {
            backend.stop();
        }
        let _ = self.events.send(VoiceEvent::Left);
    }
}

struct StreamBackend {
    pipeline: gst::Pipeline,
    webrtc: gst::Element,
    volume: Option<gst::Element>,
    source: Option<gst::Element>,
    participant_id: Option<String>,
}

impl StreamBackend {
    fn stop(&self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

fn build_up_stream(
    client_id: String,
    stream_id: String,
    out: mpsc::UnboundedSender<Value>,
    muted: bool,
    microphone: Option<&str>,
    ice: &[IceServer],
) -> Result<StreamBackend> {
    gst::init().context("could not initialise GStreamer")?;
    let pipeline = gst::Pipeline::new();
    let webrtc = make_webrtcbin(ice)?;

    let source = if std::env::var("JABU_VOICE_TEST_TONE").is_ok() {
        gst::ElementFactory::make("audiotestsrc")
            .property("is-live", true)
            .property("freq", 440.0)
            .property("volume", 0.2)
            .property_from_str("wave", "sine")
            .build()?
    } else {
        audio_source(microphone)?
    };
    let volume = gst::ElementFactory::make("volume").name("mic").build()?;
    volume.set_property("mute", muted);
    let convert = gst::ElementFactory::make("audioconvert").build()?;
    let resample = gst::ElementFactory::make("audioresample").build()?;
    let encoder = gst::ElementFactory::make("opusenc").build()?;
    let pay = gst::ElementFactory::make("rtpopuspay").build()?;
    let caps = gst::Caps::builder("application/x-rtp")
        .field("media", "audio")
        .field("encoding-name", "OPUS")
        .field("payload", 96i32)
        .build();
    let capsfilter = gst::ElementFactory::make("capsfilter")
        .property("caps", &caps)
        .build()?;

    pipeline.add_many([
        &source,
        &volume,
        &convert,
        &resample,
        &encoder,
        &pay,
        &capsfilter,
        &webrtc,
    ])?;
    gst::Element::link_many([
        &source,
        &volume,
        &convert,
        &resample,
        &encoder,
        &pay,
        &capsfilter,
    ])?;
    let sink = webrtc
        .request_pad_simple("sink_%u")
        .ok_or_else(|| anyhow!("webrtcbin has no request sink pad"))?;
    capsfilter
        .static_pad("src")
        .ok_or_else(|| anyhow!("capsfilter has no src pad"))?
        .link(&sink)?;

    connect_ice(&webrtc, &client_id, &stream_id, out);
    pipeline.set_state(gst::State::Playing)?;
    Ok(StreamBackend {
        pipeline,
        webrtc,
        volume: Some(volume),
        source: Some(source),
        participant_id: None,
    })
}

fn build_down_stream(
    client_id: String,
    stream_id: String,
    out: mpsc::UnboundedSender<Value>,
    ice: &[IceServer],
    session: &Arc<Session>,
    participant_id: Option<String>,
) -> Result<StreamBackend> {
    gst::init().context("could not initialise GStreamer")?;
    let pipeline = gst::Pipeline::new();
    let webrtc = make_webrtcbin(ice)?;
    let depay = gst::ElementFactory::make("rtpopusdepay").build()?;
    let decoder = gst::ElementFactory::make("opusdec").build()?;
    let convert = gst::ElementFactory::make("audioconvert").build()?;
    let caps = gst::Caps::builder("audio/x-raw")
        .field("format", "F32LE")
        .build();
    let capsfilter = gst::ElementFactory::make("capsfilter")
        .property("caps", &caps)
        .build()?;
    let volume = gst::ElementFactory::make("volume").build()?;
    let resample = gst::ElementFactory::make("audioresample").build()?;
    let sink = gst::ElementFactory::make("autoaudiosink").build()?;

    pipeline.add_many([
        &webrtc,
        &depay,
        &decoder,
        &convert,
        &capsfilter,
        &volume,
        &resample,
        &sink,
    ])?;
    gst::Element::link_many([
        &depay,
        &decoder,
        &convert,
        &capsfilter,
        &volume,
        &resample,
        &sink,
    ])?;

    if let Some(participant_id) = participant_id.as_ref() {
        attach_level_probe(&capsfilter, session, participant_id);
    }

    if let Some(gain) = session
        .volumes
        .read()
        .get(participant_id.as_deref().unwrap_or_default())
        .copied()
    {
        volume.set_property("volume", gain);
    }

    let pipeline_weak = pipeline.downgrade();
    let depay_sink = depay
        .static_pad("sink")
        .ok_or_else(|| anyhow!("rtpopusdepay has no sink pad"))?;
    webrtc.connect("pad-added", false, move |values| {
        let pad = values[1].get::<gst::Pad>().ok()?;
        if pad.direction() != gst::PadDirection::Src {
            return None;
        }
        let caps = pad.current_caps().or_else(|| Some(pad.query_caps(None)))?;
        let structure = caps.structure(0)?;
        if structure.name() != "application/x-rtp" {
            return None;
        }
        let _ = pad.link(&depay_sink);
        if let Some(pipeline) = pipeline_weak.upgrade() {
            let _ = pipeline.sync_children_states();
        }
        None
    });

    connect_ice(&webrtc, &client_id, &stream_id, out);
    pipeline.set_state(gst::State::Playing)?;
    Ok(StreamBackend {
        pipeline,
        webrtc,
        volume: Some(volume),
        source: None,
        participant_id,
    })
}

fn make_webrtcbin(ice: &[IceServer]) -> Result<gst::Element> {
    let webrtc = gst::ElementFactory::make("webrtcbin")
        .name("webrtc")
        .build()
        .context("missing webrtcbin (install gst-plugins-bad)")?;
    webrtc.set_property("bundle-policy", gst_webrtc::WebRTCBundlePolicy::MaxBundle);
    webrtc.set_property("latency", 100u32);
    for server in ice {
        for url in &server.urls {
            if url.starts_with("stun:") || url.starts_with("stuns:") {
                webrtc.set_property("stun-server", url.replace("stun:", "stun://"));
            } else if url.starts_with("turn:") || url.starts_with("turns:") {
                if let Some(turn) = turn_url(url, server) {
                    webrtc.set_property("turn-server", turn);
                }
            }
        }
    }
    Ok(webrtc)
}

fn turn_url(url: &str, server: &IceServer) -> Option<String> {
    let (scheme, rest) = url.split_once(':')?;
    let Some(username) = server.username.as_deref() else {
        return None;
    };
    let Some(credential) = server.credential.as_deref() else {
        return None;
    };
    Some(format!("{scheme}://{username}:{credential}@{rest}"))
}

fn parse_ice_servers(value: &Value) -> Vec<IceServer> {
    let Some(servers) = value
        .get("rtcConfiguration")
        .and_then(|configuration| configuration.get("iceServers"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    servers
        .iter()
        .filter_map(|server| {
            let urls = match server.get("urls") {
                Some(Value::String(url)) => vec![url.clone()],
                Some(Value::Array(urls)) => urls
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                _ => return None,
            };
            if urls.is_empty() {
                return None;
            }
            Some(IceServer {
                urls,
                username: server
                    .get("username")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                credential: server
                    .get("credential")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

fn connect_ice(
    webrtc: &gst::Element,
    client_id: &str,
    stream_id: &str,
    out: mpsc::UnboundedSender<Value>,
) {
    let id = stream_id.to_string();
    let source = client_id.to_string();
    webrtc.connect("on-ice-candidate", false, move |values| {
        let mline = values[1].get::<u32>().ok()?;
        let candidate = values[2].get::<String>().ok()?;
        let _ = out.send(json!({
            "type": "ice",
            "source": source,
            "id": id,
            "candidate": {
                "candidate": candidate,
                "sdpMid": mline.to_string(),
                "sdpMLineIndex": mline,
            },
        }));
        None
    });
}

fn attach_level_probe(capsfilter: &gst::Element, session: &Arc<Session>, participant_id: &str) {
    let Some(pad) = capsfilter.static_pad("src") else {
        return;
    };
    let weak = Arc::downgrade(session);
    let participant_id = participant_id.to_string();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        if let Some(buffer) = info.buffer() {
            if let Ok(map) = buffer.map_readable() {
                let data = map.as_slice();
                let mut sum = 0.0f64;
                let mut count = 0usize;
                for chunk in data.chunks_exact(4) {
                    let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    sum += f64::from(sample * sample);
                    count += 1;
                }
                if count > 0 && (sum / count as f64).sqrt() > 0.01 {
                    if let Some(session) = weak.upgrade() {
                        session.note_voice(&participant_id);
                    }
                }
            }
        }
        gst::PadProbeReturn::Ok
    });
}

fn audio_source(device_id: Option<&str>) -> Result<gst::Element> {
    if let Some(device_id) = device_id {
        for (device, audio) in audio_devices_with_id() {
            if audio.id == device_id {
                if let Ok(element) = device.create_element(None) {
                    return Ok(element);
                }
            }
        }
    }
    Ok(gst::ElementFactory::make("autoaudiosrc").build()?)
}

fn default_endpoint(base: &str) -> String {
    if let Some(rest) = base.strip_prefix("https://") {
        let host = rest.split('/').next().unwrap_or(rest);
        format!("wss://{host}/ws")
    } else if let Some(rest) = base.strip_prefix("http://") {
        let host = rest.split('/').next().unwrap_or(rest);
        format!("ws://{host}/ws")
    } else {
        format!("wss://{base}/ws")
    }
}
