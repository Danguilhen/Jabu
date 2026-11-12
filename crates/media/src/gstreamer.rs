//! GStreamer `webrtcbin` media backend (Linux desktop).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_sdp as gst_sdp;
use gstreamer_webrtc as gst_webrtc;
use parking_lot::{Mutex, RwLock};
use tokio::sync::{broadcast, oneshot};
use xmpp_core::jingle::{AudioDevice, CallKind, CallPreview, CameraDevice, MediaEvent, MediaPort};

use crate::jingle_sdp;

const CAMERA_PAYLOAD: i32 = 97;
const SCREEN_PAYLOAD: i32 = 98;

/// One ICE server (STUN or TURN) handed to the media backend.
#[derive(Debug, Clone, Default)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

struct GStreamerCall {
    pipeline: gst::Pipeline,
    webrtc: gst::Element,
    mic: gst::Element,
    mic_src: Arc<RwLock<Option<gst::Element>>>,
    mic_muted: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    content_names: Arc<RwLock<Vec<String>>>,
    video_transceiver: Arc<AtomicBool>,
    video_input: Arc<RwLock<Option<gst::Element>>>,
    video_src: Arc<RwLock<Option<gst::Element>>>,
    video_device: Arc<RwLock<Option<String>>>,
    preview: Arc<RwLock<Option<CallPreview>>>,
    remote_preview: Arc<RwLock<Option<CallPreview>>>,
    screen_transceiver: Arc<AtomicBool>,
    screen_input: Arc<RwLock<Option<gst::Element>>>,
    screen_src: Arc<RwLock<Option<gst::Element>>>,
    screen_preview: Arc<RwLock<Option<CallPreview>>>,
    remote_screen: Arc<RwLock<Option<CallPreview>>>,
    speaker: Arc<RwLock<Option<gst::Element>>>,
    speaker_sink: Arc<RwLock<Option<gst::Element>>>,
    speaker_muted: Arc<AtomicBool>,
    content_volumes: Arc<RwLock<HashMap<String, gst::Element>>>,
    content_levels: Arc<RwLock<HashMap<String, Instant>>>,
    screen_contents: Arc<RwLock<HashSet<String>>>,
    receivers: HashMap<String, ConferenceReceiver>,
    screen_mline: Arc<RwLock<Option<u32>>>,
    pending_candidates: Arc<Mutex<Vec<(u32, String)>>>,
    local_sdp: Arc<RwLock<Option<String>>>,
    remote_sdp: Arc<RwLock<Option<String>>>,
}

struct ConferenceReceiver {
    pipeline: gst::Pipeline,
    webrtc: gst::Element,
}

struct ScreenCall {
    pipeline: gst::Pipeline,
    webrtc: gst::Element,
    remote_sdp: Arc<RwLock<Option<String>>>,
}

struct BuiltCall {
    pipeline: gst::Pipeline,
    webrtc: gst::Element,
    mic: gst::Element,
    mic_src: Arc<RwLock<Option<gst::Element>>>,
    mic_muted: Arc<AtomicBool>,
    content_names: Arc<RwLock<Vec<String>>>,
    video_transceiver: Arc<AtomicBool>,
    video_input: Arc<RwLock<Option<gst::Element>>>,
    video_src: Arc<RwLock<Option<gst::Element>>>,
    video_device: Arc<RwLock<Option<String>>>,
    preview: Arc<RwLock<Option<CallPreview>>>,
    remote_preview: Arc<RwLock<Option<CallPreview>>>,
    screen_transceiver: Arc<AtomicBool>,
    screen_input: Arc<RwLock<Option<gst::Element>>>,
    screen_src: Arc<RwLock<Option<gst::Element>>>,
    screen_preview: Arc<RwLock<Option<CallPreview>>>,
    remote_screen: Arc<RwLock<Option<CallPreview>>>,
    speaker: Arc<RwLock<Option<gst::Element>>>,
    speaker_sink: Arc<RwLock<Option<gst::Element>>>,
    speaker_muted: Arc<AtomicBool>,
    content_volumes: Arc<RwLock<HashMap<String, gst::Element>>>,
    content_levels: Arc<RwLock<HashMap<String, Instant>>>,
    screen_contents: Arc<RwLock<HashSet<String>>>,
    screen_mline: Arc<RwLock<Option<u32>>>,
    pending_candidates: Arc<Mutex<Vec<(u32, String)>>>,
    local_sdp: Arc<RwLock<Option<String>>>,
    remote_sdp: Arc<RwLock<Option<String>>>,
}

/// GStreamer-backed media implementation.
pub struct GStreamerMediaPort {
    calls: Mutex<HashMap<String, GStreamerCall>>,
    /// ICE candidates that arrived before the media call existed. The caller
    /// flushes its candidates before sending `session-initiate`, so without
    /// this buffer the callee would drop the first (and usually only host)
    /// candidate.
    pending_remote: Mutex<HashMap<String, Vec<(u32, String)>>>,
    events: broadcast::Sender<MediaEvent>,
    ice_servers: Mutex<Vec<IceServer>>,
    preferred_camera: Mutex<Option<String>>,
    preferred_audio: Mutex<Option<String>>,
    screen_node: Mutex<Option<u32>>,
    screen_session: Mutex<Option<crate::screencast::ScreenCastSession>>,
    screen_calls: Mutex<HashMap<String, ScreenCall>>,
    story_preview: Arc<RwLock<Option<CallPreview>>>,
    story_pipeline: Mutex<Option<gst::Pipeline>>,
}

impl GStreamerMediaPort {
    pub fn new(ice_servers: Vec<IceServer>) -> Result<Arc<Self>> {
        ensure_plugin_path();
        gst::init().map_err(|error| anyhow!("GStreamer init failed: {error}"))?;
        register_optional_plugins();
        let (events, _) = broadcast::channel(64);
        Ok(Arc::new(Self {
            calls: Mutex::new(HashMap::new()),
            pending_remote: Mutex::new(HashMap::new()),
            events,
            ice_servers: Mutex::new(ice_servers),
            preferred_camera: Mutex::new(None),
            preferred_audio: Mutex::new(None),
            screen_node: Mutex::new(None),
            screen_session: Mutex::new(None),
            screen_calls: Mutex::new(HashMap::new()),
            story_preview: Arc::new(RwLock::new(None)),
            story_pipeline: Mutex::new(None),
        }))
    }

    pub fn set_ice_servers(&self, servers: Vec<IceServer>) {
        tracing::debug!(
            count = servers.len(),
            urls = ?servers.iter().flat_map(|server| server.urls.iter()).collect::<Vec<_>>(),
            "call ICE servers updated"
        );
        *self.ice_servers.lock() = servers;
    }

    /// Apply ICE candidates that arrived before the call was inserted.
    fn flush_remote_candidates(&self, sid: &str, webrtc: &gst::Element) {
        let pending = self.pending_remote.lock().remove(sid);
        if let Some(pending) = pending {
            tracing::debug!(
                "applying {} buffered ICE candidate(s) to call {sid}",
                pending.len()
            );
            for (mline, candidate) in pending {
                webrtc.emit_by_name::<()>("add-ice-candidate", &[&mline, &candidate]);
            }
        }
    }

    fn stun_turn(&self) -> (Option<String>, Option<String>) {
        // Test/deployment override: a TURN endpoint that is not advertised by
        // the XMPP server (for example a LAN address during local testing).
        if let Ok(turn) = std::env::var("JABU_TURN_URL") {
            let username = std::env::var("JABU_TURN_USER").ok();
            let credential = std::env::var("JABU_TURN_PASSWORD").ok();
            let stun = std::env::var("JABU_STUN_URL").ok();
            return (
                stun,
                Some(with_credentials(
                    &turn,
                    username.as_deref(),
                    credential.as_deref(),
                )),
            );
        }
        let servers = self.ice_servers.lock();
        let mut stun = None;
        let mut turn = None;
        for server in servers.iter() {
            for url in &server.urls {
                if url.starts_with("stun:") || url.starts_with("stuns:") {
                    if stun.is_none() {
                        stun = Some(url.clone());
                    }
                } else if url.starts_with("turn:") || url.starts_with("turns:") {
                    if turn.is_none() {
                        turn = Some(with_credentials(
                            url,
                            server.username.as_deref(),
                            server.credential.as_deref(),
                        ));
                    }
                }
            }
        }
        (stun, turn)
    }

    async fn ensure_screen_node(&self) -> Result<()> {
        if self.screen_node.lock().is_some() {
            return Ok(());
        }
        if std::env::var("JABU_SCREEN_SOURCE").ok().as_deref() == Some("test") {
            return Ok(());
        }
        if let Ok(node) = std::env::var("JABU_SCREEN_NODE") {
            if let Ok(node) = node.parse::<u32>() {
                *self.screen_node.lock() = Some(node);
                return Ok(());
            }
        }
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            let session = crate::screencast::request_screen_cast().await?;
            *self.screen_node.lock() = Some(session.node);
            *self.screen_session.lock() = Some(session);
        }
        Ok(())
    }

    fn create_screen_source(&self) -> Result<gst::Element> {
        if std::env::var("JABU_SCREEN_SOURCE").ok().as_deref() == Some("test") {
            let source = gst::ElementFactory::make("videotestsrc")
                .property("is-live", true)
                .property("do-timestamp", true)
                .build()?;
            return Ok(source);
        }
        let node = *self.screen_node.lock();
        // A portal session comes with its own PipeWire descriptor; a node picked
        // by hand (`JABU_SCREEN_NODE`) is on the default connection instead.
        // Only the Linux screencast backend exposes that descriptor.
        #[cfg(target_os = "linux")]
        {
            if let Some(session) = self.screen_session.lock().as_ref() {
                let source = gst::ElementFactory::make("pipewiresrc").build()?;
                source.set_property("path", session.node.to_string());
                source.set_property("fd", std::os::fd::AsRawFd::as_raw_fd(&session.fd));
                source.set_property("always-copy", true);
                source.set_property("do-timestamp", true);
                return Ok(source);
            }
        }
        if let Some(node) = node {
            let source = gst::ElementFactory::make("pipewiresrc").build()?;
            source.set_property("path", node.to_string());
            source.set_property("always-copy", true);
            source.set_property("do-timestamp", true);
            return Ok(source);
        }
        #[cfg(target_os = "linux")]
        {
            let source = gst::ElementFactory::make("ximagesrc").build()?;
            source.set_property("use-damage", false);
            source.set_property("show-pointer", true);
            Ok(source)
        }
        #[cfg(not(target_os = "linux"))]
        anyhow::bail!("screen capture is not supported on this platform")
    }

    fn create_audio_source(&self) -> Result<gst::Element> {
        if std::env::var_os("JABU_AUDIO_TEST_TONE").is_some()
            || std::env::var_os("JABU_VOICE_TEST_TONE").is_some()
        {
            let source = gst::ElementFactory::make("audiotestsrc").build()?;
            source.set_property("is-live", true);
            return Ok(source);
        }
        let preferred = self.preferred_audio.lock().clone();
        Self::create_audio_source_for(preferred.as_deref())
    }

    fn create_audio_source_for(device_id: Option<&str>) -> Result<gst::Element> {
        if let Some(device_id) = device_id {
            for (device, audio) in audio_devices_with_id() {
                if audio.id == device_id {
                    if let Ok(element) = device.create_element(None) {
                        return Ok(element);
                    }
                }
            }
            if gst::ElementFactory::find("pulsesrc").is_some() {
                let source = gst::ElementFactory::make("pulsesrc").build()?;
                source.set_property("device", device_id);
                return Ok(source);
            }
        }
        Ok(gst::ElementFactory::make("autoaudiosrc").build()?)
    }

    fn create_camera_source(&self) -> Result<(gst::Element, Option<String>)> {
        self.create_camera_source_for(None)
    }

    fn create_camera_source_for(
        &self,
        device_id: Option<&str>,
    ) -> Result<(gst::Element, Option<String>)> {
        let preferred = device_id
            .map(str::to_string)
            .or_else(|| self.preferred_camera.lock().clone());
        if let Some(preferred) = preferred {
            for (device, camera) in camera_devices_with_id() {
                if camera.id == preferred {
                    if let Ok(element) = device.create_element(None) {
                        return Ok((element, Some(camera.id)));
                    }
                }
            }
        }
        let element = gst::ElementFactory::make("autovideosrc").build()?;
        Ok((element, None))
    }

    fn start_story_camera_impl(&self, device_id: Option<&str>) -> Result<()> {
        self.stop_story_camera_impl();
        let (source, _) = self.create_camera_source_for(device_id)?;
        let pipeline = gst::Pipeline::new();
        let convert = gst::ElementFactory::make("videoconvert").build()?;
        let scale = gst::ElementFactory::make("videoscale").build()?;
        let caps = gst::Caps::builder("video/x-raw")
            .field("format", "RGBA")
            .field("width", 480i32)
            .field("height", 640i32)
            .build();
        let filter = gst::ElementFactory::make("capsfilter")
            .property("caps", &caps)
            .build()?;
        let appsink = make_preview_appsink(
            self.story_preview.clone(),
            std::time::Duration::from_millis(200),
            80,
        )?;
        pipeline.add_many([&source, &convert, &scale, &filter, &appsink])?;
        gst::Element::link_many([&source, &convert, &scale, &filter, &appsink])?;
        pipeline.set_state(gst::State::Playing)?;
        *self.story_pipeline.lock() = Some(pipeline);
        Ok(())
    }

    fn stop_story_camera_impl(&self) {
        if let Some(pipeline) = self.story_pipeline.lock().take() {
            let _ = pipeline.set_state(gst::State::Null);
        }
        *self.story_preview.write() = None;
    }

    fn story_camera_frame_impl(&self) -> Option<CallPreview> {
        self.story_preview.read().clone()
    }

    fn build(&self) -> Result<BuiltCall> {
        let pipeline = gst::Pipeline::new();
        let webrtc = gst::ElementFactory::make("webrtcbin")
            .name("webrtc")
            .build()
            .context("missing webrtcbin: the GStreamer plugins are incomplete (build and run the app from `nix develop`)")?;
        webrtc.set_property("bundle-policy", gst_webrtc::WebRTCBundlePolicy::MaxBundle);
        webrtc.set_property("latency", 100u32);
        let (stun, turn) = self.stun_turn();
        if let Some(stun) = stun {
            webrtc.set_property("stun-server", stun);
        }
        if let Some(turn) = turn {
            webrtc.set_property("turn-server", turn);
        }
        if std::env::var_os("JABU_ICE_RELAY_ONLY").is_some() {
            // Test hook: force TURN so a relay can be verified on one LAN.
            webrtc.set_property(
                "ice-transport-policy",
                gst_webrtc::WebRTCICETransportPolicy::Relay,
            );
        }
        pipeline.add(&webrtc)?;

        let src = self.create_audio_source()?;
        let mic_muted = Arc::new(AtomicBool::new(false));
        attach_child_mute(&src, mic_muted.clone());
        let mic = gst::ElementFactory::make("volume").name("mic").build()?;
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
        pipeline.add_many([&src, &mic, &convert, &resample, &encoder, &pay, &capsfilter])?;
        gst::Element::link_many([&src, &mic, &convert, &resample, &encoder, &pay, &capsfilter])?;
        let sink = webrtc
            .request_pad_simple("sink_0")
            .or_else(|| webrtc.request_pad_simple("sink_%u"))
            .ok_or_else(|| anyhow!("webrtcbin has no request sink pad"))?;
        capsfilter
            .static_pad("src")
            .ok_or_else(|| anyhow!("capsfilter has no src pad"))?
            .link(&sink)?;

        let content_names = Arc::new(RwLock::new(Vec::new()));
        let video_transceiver = Arc::new(AtomicBool::new(false));
        let video_input = Arc::new(RwLock::new(None));
        let video_src = Arc::new(RwLock::new(None));
        let video_device = Arc::new(RwLock::new(None));
        let preview = Arc::new(RwLock::new(None));
        let remote_preview = Arc::new(RwLock::new(None));
        let screen_transceiver = Arc::new(AtomicBool::new(false));
        let screen_input = Arc::new(RwLock::new(None));
        let screen_src = Arc::new(RwLock::new(None));
        let screen_preview = Arc::new(RwLock::new(None));
        let remote_screen = Arc::new(RwLock::new(None));
        let speaker = Arc::new(RwLock::new(None));
        let speaker_sink = Arc::new(RwLock::new(None));
        let speaker_muted = Arc::new(AtomicBool::new(false));
        let content_volumes = Arc::new(RwLock::new(HashMap::new()));
        let content_levels = Arc::new(RwLock::new(HashMap::new()));
        let screen_contents = Arc::new(RwLock::new(HashSet::new()));
        let screen_mline = Arc::new(RwLock::new(None));
        let pending_candidates = Arc::new(Mutex::new(Vec::new()));
        let local_sdp = Arc::new(RwLock::new(None));
        let remote_sdp = Arc::new(RwLock::new(None));

        Ok(BuiltCall {
            pipeline,
            webrtc,
            mic,
            mic_src: Arc::new(RwLock::new(Some(src.clone()))),
            mic_muted,
            content_names,
            video_transceiver,
            video_input,
            video_src,
            video_device,
            preview,
            remote_preview,
            screen_transceiver,
            screen_input,
            screen_src,
            screen_preview,
            remote_screen,
            speaker,
            speaker_sink,
            speaker_muted,
            content_volumes,
            content_levels,
            screen_contents,
            screen_mline,
            pending_candidates,
            local_sdp,
            remote_sdp,
        })
    }

    async fn create_screen_call_offer(&self, sid: &str) -> Result<String> {
        if std::env::var("JABU_SCREEN_SOURCE").ok().as_deref() != Some("test") {
            self.ensure_screen_node().await?;
        }
        let source = self.create_screen_source()?;
        let (stun, turn) = self.stun_turn();
        let pipeline = gst::Pipeline::new();
        let webrtc = gst::ElementFactory::make("webrtcbin")
            .name("webrtc-screen")
            .build()
            .context("missing webrtcbin: the GStreamer plugins are incomplete (build and run the app from `nix develop`)")?;
        webrtc.set_property("bundle-policy", gst_webrtc::WebRTCBundlePolicy::MaxBundle);
        webrtc.set_property("latency", 100u32);
        if let Some(stun) = stun {
            webrtc.set_property("stun-server", stun);
        }
        if let Some(turn) = turn {
            webrtc.set_property("turn-server", turn);
        }
        pipeline.add(&webrtc)?;
        add_screen_transceiver(&webrtc)?;
        let pad = sink_pad_for_mline(&webrtc, 0)
            .or_else(|| webrtc.request_pad_simple("sink_0"))
            .ok_or_else(|| anyhow!("webrtcbin has no sink pad for the screen"))?;
        let preview = Arc::new(RwLock::new(None));
        attach_video_branch(
            &pipeline,
            &preview,
            pad,
            &screen_raw_caps(),
            (480, 270),
            SCREEN_PAYLOAD,
            Some(&source),
        )?;

        let content_names = Arc::new(RwLock::new(Vec::new()));
        let pending_candidates = Arc::new(Mutex::new(Vec::new()));
        connect_screen_sender(
            &webrtc,
            self.events.clone(),
            sid,
            content_names.clone(),
            pending_candidates.clone(),
        );

        pipeline.set_state(gst::State::Playing)?;
        let sdp = create_offer(&webrtc).await?;
        update_content_names(&sdp, &content_names);
        flush_pending_candidates(&self.events, sid, &content_names, &pending_candidates);
        let jingle =
            jingle_sdp::sdp_to_jingle(&sdp, xmpp_parsers::jingle::Action::SessionInitiate, sid)?;
        self.screen_calls.lock().insert(
            sid.to_string(),
            ScreenCall {
                pipeline,
                webrtc,
                remote_sdp: Arc::new(RwLock::new(None)),
            },
        );
        Ok(jingle)
    }

    async fn apply_screen_call_answer(&self, sid: &str, answer: &str) -> Result<()> {
        let (webrtc, pipeline, remote_slot) = {
            let calls = self.screen_calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown screen call {sid}"))?;
            (
                call.webrtc.clone(),
                call.pipeline.clone(),
                call.remote_sdp.clone(),
            )
        };
        let remote = jingle_sdp::jingle_to_sdp(answer)?;
        set_remote_description(&webrtc, &remote, gst_webrtc::WebRTCSDPType::Answer).await?;
        *remote_slot.write() = Some(remote);
        pipeline.set_state(gst::State::Playing)?;
        Ok(())
    }

    async fn add_screen_call_candidate(&self, sid: &str, jingle: &str) -> Result<()> {
        let webrtc = {
            let calls = self.screen_calls.lock();
            calls
                .get(sid)
                .map(|call| call.webrtc.clone())
                .ok_or_else(|| anyhow!("unknown screen call {sid}"))?
        };
        for (mline, candidate) in jingle_sdp::candidates_from_jingle(jingle)? {
            webrtc.emit_by_name::<()>("add-ice-candidate", &[&mline, &candidate]);
        }
        Ok(())
    }

    fn stop_screen_call(&self, sid: &str) {
        let call = self.screen_calls.lock().remove(sid);
        if let Some(call) = call {
            let _ = call.pipeline.set_state(gst::State::Null);
        }
    }

    async fn accept_isolated_content(
        &self,
        sid: &str,
        content_add: &str,
        key: &str,
    ) -> Result<String> {
        let (preview, screen, speaker, speaker_sink, muted, volumes, levels, slides) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            let slides = call.screen_contents.read().contains(key);
            (
                call.remote_preview.clone(),
                call.remote_screen.clone(),
                call.speaker.clone(),
                call.speaker_sink.clone(),
                call.speaker_muted.clone(),
                call.content_volumes.clone(),
                call.content_levels.clone(),
                slides,
            )
        };

        let media = jingle_sdp::first_content_media(content_add)?;
        let content_name = jingle_sdp::first_content_name(content_add)?;
        let remote = jingle_sdp::media_to_sdp(&media)?;
        let (stun, turn) = self.stun_turn();

        let pipeline = gst::Pipeline::new();
        let webrtc = gst::ElementFactory::make("webrtcbin")
            .name("webrtc-recv")
            .build()
            .context("missing webrtcbin: the GStreamer plugins are incomplete (build and run the app from `nix develop`)")?;
        webrtc.set_property("bundle-policy", gst_webrtc::WebRTCBundlePolicy::MaxBundle);
        webrtc.set_property("latency", 100u32);
        if let Some(stun) = stun {
            webrtc.set_property("stun-server", stun);
        }
        if let Some(turn) = turn {
            webrtc.set_property("turn-server", turn);
        }
        pipeline.add(&webrtc)?;

        attach_isolated_receiver(
            &pipeline,
            &webrtc,
            key,
            &content_name,
            slides,
            &preview,
            &screen,
            &speaker,
            &speaker_sink,
            &muted,
            &volumes,
            &levels,
            &self.events,
            sid,
        );

        pipeline.set_state(gst::State::Playing)?;
        set_remote_description(&webrtc, &remote, gst_webrtc::WebRTCSDPType::Offer).await?;
        let answer = create_answer(&webrtc).await?;
        let content = jingle_sdp::sdp_to_jingle_media_at(
            &answer,
            xmpp_parsers::jingle::Action::ContentAccept,
            sid,
            0,
        )?;

        let mut calls = self.calls.lock();
        let call = calls
            .get_mut(sid)
            .ok_or_else(|| anyhow!("unknown call {sid}"))?;
        if let Some(old) = call
            .receivers
            .insert(key.to_string(), ConferenceReceiver { pipeline, webrtc })
        {
            let _ = old.pipeline.set_state(gst::State::Null);
        }
        Ok(content)
    }
}

#[async_trait]
impl MediaPort for GStreamerMediaPort {
    async fn create_offer(&self, sid: &str, kind: CallKind) -> Result<String> {
        let BuiltCall {
            pipeline,
            webrtc,
            mic,
            mic_src,
            mic_muted,
            content_names,
            video_transceiver,
            video_input,
            video_src,
            video_device,
            preview,
            remote_preview,
            screen_transceiver,
            screen_input,
            screen_src,
            screen_preview,
            remote_screen,
            speaker,
            speaker_sink,
            speaker_muted,
            content_volumes,
            content_levels,
            screen_contents,
            screen_mline,
            pending_candidates,
            local_sdp,
            remote_sdp,
        } = self.build()?;
        if kind == CallKind::Video {
            add_video_transceiver(&webrtc)?;
            video_transceiver.store(true, Ordering::Relaxed);
        }
        let connected = Arc::new(AtomicBool::new(false));
        let first_offer = Arc::new(AtomicBool::new(false));
        let (offer_tx, mut offer_rx) = tokio::sync::mpsc::unbounded_channel();
        connect_common(
            &pipeline,
            &webrtc,
            &self.events,
            sid,
            content_names.clone(),
            remote_preview.clone(),
            remote_screen.clone(),
            speaker.clone(),
            speaker_sink.clone(),
            speaker_muted.clone(),
            content_volumes.clone(),
            content_levels.clone(),
            screen_contents.clone(),
            pending_candidates.clone(),
            connected.clone(),
            first_offer,
            Some(offer_tx),
        );

        pipeline.set_state(gst::State::Playing)?;
        let sdp = tokio::time::timeout(Duration::from_secs(15), offer_rx.recv())
            .await
            .map_err(|_| anyhow!("timed out waiting for the local offer"))?
            .ok_or_else(|| anyhow!("local offer channel closed"))?;
        update_content_names(&sdp, &content_names);
        flush_pending_candidates(&self.events, sid, &content_names, &pending_candidates);
        *local_sdp.write() = Some(sdp.clone());
        let jingle =
            jingle_sdp::sdp_to_jingle(&sdp, xmpp_parsers::jingle::Action::SessionInitiate, sid)?;
        let flush_webrtc = webrtc.clone();
        self.calls.lock().insert(
            sid.to_string(),
            GStreamerCall {
                pipeline,
                webrtc,
                mic,
                mic_src,
                mic_muted,
                connected,
                content_names,
                video_transceiver,
                video_input,
                video_src,
                video_device,
                preview,
                remote_preview,
                screen_transceiver,
                screen_input,
                screen_src,
                screen_preview,
                remote_screen,
                speaker,
                speaker_sink,
                speaker_muted,
                content_volumes,
                content_levels,
                screen_contents,
                receivers: HashMap::new(),
                screen_mline,
                pending_candidates,
                local_sdp,
                remote_sdp,
            },
        );
        self.flush_remote_candidates(sid, &flush_webrtc);
        Ok(jingle)
    }

    async fn accept_offer(&self, sid: &str, kind: CallKind, offer: &str) -> Result<String> {
        let BuiltCall {
            pipeline,
            webrtc,
            mic,
            mic_src,
            mic_muted,
            content_names,
            video_transceiver,
            video_input,
            video_src,
            video_device,
            preview,
            remote_preview,
            screen_transceiver,
            screen_input,
            screen_src,
            screen_preview,
            remote_screen,
            speaker,
            speaker_sink,
            speaker_muted,
            content_volumes,
            content_levels,
            screen_contents,
            screen_mline,
            pending_candidates,
            local_sdp,
            remote_sdp,
        } = self.build()?;
        let connected = Arc::new(AtomicBool::new(false));
        connect_common(
            &pipeline,
            &webrtc,
            &self.events,
            sid,
            content_names.clone(),
            remote_preview.clone(),
            remote_screen.clone(),
            speaker.clone(),
            speaker_sink.clone(),
            speaker_muted.clone(),
            content_volumes.clone(),
            content_levels.clone(),
            screen_contents.clone(),
            pending_candidates.clone(),
            connected.clone(),
            Arc::new(AtomicBool::new(false)),
            None,
        );

        pipeline.set_state(gst::State::Playing)?;
        let remote = jingle_sdp::jingle_to_sdp(offer)?;
        set_remote_description(&webrtc, &remote, gst_webrtc::WebRTCSDPType::Offer).await?;
        if kind == CallKind::Video {
            attach_video_sender(&pipeline, &webrtc, &preview, &video_input, &remote)?;
        }
        let sdp = create_answer(&webrtc).await?;
        update_content_names(&sdp, &content_names);
        flush_pending_candidates(&self.events, sid, &content_names, &pending_candidates);
        *remote_sdp.write() = Some(remote);
        *local_sdp.write() = Some(sdp.clone());
        let jingle =
            jingle_sdp::sdp_to_jingle(&sdp, xmpp_parsers::jingle::Action::SessionAccept, sid)?;
        let flush_webrtc = webrtc.clone();
        self.calls.lock().insert(
            sid.to_string(),
            GStreamerCall {
                pipeline,
                webrtc,
                mic,
                mic_src,
                mic_muted,
                connected,
                content_names,
                video_transceiver,
                video_input,
                video_src,
                video_device,
                preview,
                remote_preview,
                screen_transceiver,
                screen_input,
                screen_src,
                screen_preview,
                remote_screen,
                speaker,
                speaker_sink,
                speaker_muted,
                content_volumes,
                content_levels,
                screen_contents,
                receivers: HashMap::new(),
                screen_mline,
                pending_candidates,
                local_sdp,
                remote_sdp,
            },
        );
        self.flush_remote_candidates(sid, &flush_webrtc);
        Ok(jingle)
    }

    async fn apply_answer(&self, sid: &str, answer: &str) -> Result<()> {
        let (webrtc, pipeline, remote_slot, local_slot, preview, video_input, content_names) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (
                call.webrtc.clone(),
                call.pipeline.clone(),
                call.remote_sdp.clone(),
                call.local_sdp.clone(),
                call.preview.clone(),
                call.video_input.clone(),
                call.content_names.clone(),
            )
        };
        let remote = jingle_sdp::jingle_to_sdp(answer)?;
        update_content_names(&remote, &content_names);
        set_remote_description(&webrtc, &remote, gst_webrtc::WebRTCSDPType::Answer).await?;
        *remote_slot.write() = Some(remote);
        pipeline.set_state(gst::State::Playing)?;
        if let Some(local) = local_slot.read().clone() {
            if jingle_sdp::sdp_media_index(&local, "video").is_some() {
                attach_video_sender(&pipeline, &webrtc, &preview, &video_input, &local)?;
            }
        }
        Ok(())
    }

    async fn add_video_offer(&self, sid: &str) -> Result<String> {
        let (webrtc, video_transceiver, content_names, pending_candidates, events, local_sdp) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (
                call.webrtc.clone(),
                call.video_transceiver.clone(),
                call.content_names.clone(),
                call.pending_candidates.clone(),
                self.events.clone(),
                call.local_sdp.clone(),
            )
        };
        if !video_transceiver.swap(true, Ordering::Relaxed) {
            add_video_transceiver(&webrtc)?;
        }
        let sdp = create_offer(&webrtc).await?;
        update_content_names(&sdp, &content_names);
        flush_pending_candidates(&events, sid, &content_names, &pending_candidates);
        *local_sdp.write() = Some(sdp.clone());
        jingle_sdp::sdp_to_jingle_media(
            &sdp,
            xmpp_parsers::jingle::Action::ContentAdd,
            sid,
            "video",
        )
    }

    async fn accept_video_add(&self, sid: &str, content_add: &str) -> Result<String> {
        let (
            pipeline,
            webrtc,
            preview,
            video_input,
            content_names,
            pending_candidates,
            events,
            local_sdp,
            remote_slot,
        ) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (
                call.pipeline.clone(),
                call.webrtc.clone(),
                call.preview.clone(),
                call.video_input.clone(),
                call.content_names.clone(),
                call.pending_candidates.clone(),
                self.events.clone(),
                call.local_sdp.clone(),
                call.remote_sdp.clone(),
            )
        };
        let remote = remote_slot
            .read()
            .clone()
            .ok_or_else(|| anyhow!("no remote description for call {sid}"))?;
        let video = jingle_sdp::first_content_media(content_add)?;
        let merged = jingle_sdp::merge_sdp(&remote, &[video], true)?;
        set_remote_description(&webrtc, &merged, gst_webrtc::WebRTCSDPType::Offer).await?;
        attach_video_sender(&pipeline, &webrtc, &preview, &video_input, &merged)?;

        let sdp = create_answer(&webrtc).await?;
        update_content_names(&sdp, &content_names);
        flush_pending_candidates(&events, sid, &content_names, &pending_candidates);
        *remote_slot.write() = Some(merged);
        *local_sdp.write() = Some(sdp.clone());
        jingle_sdp::sdp_to_jingle_media(
            &sdp,
            xmpp_parsers::jingle::Action::ContentAccept,
            sid,
            "video",
        )
    }

    async fn apply_video_answer(&self, sid: &str, content_accept: &str) -> Result<()> {
        let (webrtc, pipeline, local_slot, remote_slot, preview, video_input) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (
                call.webrtc.clone(),
                call.pipeline.clone(),
                call.local_sdp.clone(),
                call.remote_sdp.clone(),
                call.preview.clone(),
                call.video_input.clone(),
            )
        };
        let local = local_slot
            .read()
            .clone()
            .ok_or_else(|| anyhow!("no local offer for call {sid}"))?;
        let previous_remote = remote_slot
            .read()
            .clone()
            .ok_or_else(|| anyhow!("no remote description for call {sid}"))?;
        let audio_mid =
            jingle_sdp::sdp_first_mid(&previous_remote).unwrap_or_else(|_| "audio0".to_string());
        let audio = jingle_sdp::sdp_media_by_mid(&previous_remote, &audio_mid)
            .ok_or_else(|| anyhow!("remote description has no audio media"))?;
        let video = jingle_sdp::first_content_media(content_accept)?;
        let merged = jingle_sdp::merge_sdp(&local, &[audio, video], false)?;
        set_remote_description(&webrtc, &merged, gst_webrtc::WebRTCSDPType::Answer).await?;
        attach_video_sender(&pipeline, &webrtc, &preview, &video_input, &local)?;
        *remote_slot.write() = Some(merged);
        Ok(())
    }

    async fn create_conference_screen_offer(&self, sid: &str) -> Result<String> {
        self.create_screen_call_offer(sid).await
    }

    async fn apply_conference_screen_answer(&self, sid: &str, answer: &str) -> Result<()> {
        self.apply_screen_call_answer(sid, answer).await
    }

    async fn add_conference_screen_candidate(&self, sid: &str, jingle: &str) -> Result<()> {
        self.add_screen_call_candidate(sid, jingle).await
    }

    fn stop_conference_screen(&self, sid: &str) {
        self.stop_screen_call(sid);
        // A portal screen-cast session carries one stream: the node (and the
        // descriptor behind it) is consumed by the pipeline that just stopped.
        // Keeping it would make the next share build a source on a dead node,
        // so drop the session and ask the portal again next time.
        *self.screen_node.lock() = None;
        *self.screen_session.lock() = None;
    }

    async fn add_screen_offer(&self, sid: &str) -> Result<String> {
        let (
            webrtc,
            screen_transceiver,
            screen_mline,
            content_names,
            pending_candidates,
            events,
            local_sdp,
        ) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (
                call.webrtc.clone(),
                call.screen_transceiver.clone(),
                call.screen_mline.clone(),
                call.content_names.clone(),
                call.pending_candidates.clone(),
                self.events.clone(),
                call.local_sdp.clone(),
            )
        };
        self.ensure_screen_node().await?;
        if screen_transceiver.swap(true, Ordering::Relaxed) {
            return Err(anyhow!(
                "screen sharing is already negotiated for this call"
            ));
        }
        add_screen_transceiver(&webrtc)?;
        let sdp = create_offer(&webrtc).await?;
        update_content_names(&sdp, &content_names);
        flush_pending_candidates(&events, sid, &content_names, &pending_candidates);
        *local_sdp.write() = Some(sdp.clone());
        let index = jingle_sdp::sdp_last_media_index(&sdp, "video")
            .ok_or_else(|| anyhow!("screen offer has no video media"))?;
        *screen_mline.write() = Some(index);
        jingle_sdp::sdp_to_jingle_media_at(
            &sdp,
            xmpp_parsers::jingle::Action::ContentAdd,
            sid,
            index,
        )
    }

    async fn accept_screen_add(&self, sid: &str, content_add: &str) -> Result<String> {
        let (webrtc, content_names, pending_candidates, events, local_sdp, remote_slot) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (
                call.webrtc.clone(),
                call.content_names.clone(),
                call.pending_candidates.clone(),
                self.events.clone(),
                call.local_sdp.clone(),
                call.remote_sdp.clone(),
            )
        };
        let remote = remote_slot
            .read()
            .clone()
            .ok_or_else(|| anyhow!("no remote description for call {sid}"))?;
        let video = jingle_sdp::first_content_media(content_add)?;
        let merged = jingle_sdp::append_sdp_media(&remote, &video, true)?;
        let index = jingle_sdp::sdp_last_media_index(&merged, "video")
            .ok_or_else(|| anyhow!("screen offer has no video media"))?;
        set_remote_description(&webrtc, &merged, gst_webrtc::WebRTCSDPType::Offer).await?;
        let sdp = create_answer(&webrtc).await?;
        update_content_names(&sdp, &content_names);
        flush_pending_candidates(&events, sid, &content_names, &pending_candidates);
        *remote_slot.write() = Some(merged);
        *local_sdp.write() = Some(sdp.clone());
        jingle_sdp::sdp_to_jingle_media_at(
            &sdp,
            xmpp_parsers::jingle::Action::ContentAccept,
            sid,
            index,
        )
    }

    async fn accept_remote_content(
        &self,
        sid: &str,
        content_add: &str,
        key: &str,
        separate: bool,
    ) -> Result<String> {
        if separate {
            return self.accept_isolated_content(sid, content_add, key).await;
        }
        let (webrtc, content_names, pending_candidates, events, local_sdp, remote_slot) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (
                call.webrtc.clone(),
                call.content_names.clone(),
                call.pending_candidates.clone(),
                self.events.clone(),
                call.local_sdp.clone(),
                call.remote_sdp.clone(),
            )
        };
        let remote = remote_slot
            .read()
            .clone()
            .ok_or_else(|| anyhow!("no remote description for call {sid}"))?;
        let mid = jingle_sdp::first_content_name(content_add)?;
        let media = jingle_sdp::first_content_media(content_add)?;
        let merged = jingle_sdp::replace_sdp_media_by_mid(&remote, &mid, &media, true)?;
        update_content_names(&merged, &content_names);
        set_remote_description(&webrtc, &merged, gst_webrtc::WebRTCSDPType::Offer).await?;
        let sdp = create_answer(&webrtc).await?;
        update_content_names(&sdp, &content_names);
        flush_pending_candidates(&events, sid, &content_names, &pending_candidates);
        *remote_slot.write() = Some(merged);
        *local_sdp.write() = Some(sdp.clone());
        let index = jingle_sdp::sdp_mids(&sdp)
            .iter()
            .position(|candidate| candidate == &mid)
            .ok_or_else(|| anyhow!("answer has no media mid {mid}"))? as u32;
        jingle_sdp::sdp_to_jingle_media_at(
            &sdp,
            xmpp_parsers::jingle::Action::ContentAccept,
            sid,
            index,
        )
    }

    async fn apply_screen_answer(&self, sid: &str, content_accept: &str) -> Result<()> {
        let (webrtc, pipeline, local_slot, remote_slot, screen_preview, screen_input, screen_mline) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (
                call.webrtc.clone(),
                call.pipeline.clone(),
                call.local_sdp.clone(),
                call.remote_sdp.clone(),
                call.screen_preview.clone(),
                call.screen_input.clone(),
                call.screen_mline.clone(),
            )
        };
        let local = local_slot
            .read()
            .clone()
            .ok_or_else(|| anyhow!("no local screen offer for call {sid}"))?;
        let previous_remote = remote_slot
            .read()
            .clone()
            .ok_or_else(|| anyhow!("no remote description for call {sid}"))?;
        let screen_mid = jingle_sdp::first_content_name(content_accept)?;
        let screen = jingle_sdp::first_content_media(content_accept)?;
        let mut merged = local.clone();
        for mid in jingle_sdp::sdp_mids(&previous_remote) {
            if let Some(block) = jingle_sdp::sdp_media_by_mid(&previous_remote, &mid) {
                merged = jingle_sdp::replace_sdp_media_by_mid(&merged, &mid, &block, false)?;
            }
        }
        merged = jingle_sdp::replace_sdp_media_by_mid(&merged, &screen_mid, &screen, false)?;
        set_remote_description(&webrtc, &merged, gst_webrtc::WebRTCSDPType::Answer).await?;
        *remote_slot.write() = Some(merged);
        let index = screen_mline
            .read()
            .or_else(|| jingle_sdp::sdp_last_media_index(&local, "video"))
            .ok_or_else(|| anyhow!("screen offer has no video media"))?;
        let pad = sink_pad_for_mline(&webrtc, index)
            .or_else(|| webrtc.request_pad_simple(&format!("sink_{index}")))
            .ok_or_else(|| anyhow!("webrtcbin has no sink pad for screen m-line {index}"))?;
        let input = attach_video_branch(
            &pipeline,
            &screen_preview,
            pad,
            &screen_raw_caps(),
            (480, 270),
            SCREEN_PAYLOAD,
            None,
        )?;
        *screen_input.write() = Some(input);
        Ok(())
    }

    async fn set_screen(&self, sid: &str, enabled: bool) -> Result<()> {
        let (pipeline, input, src, src_slot, preview) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            let input = call.screen_input.read().clone();
            let src = call.screen_src.read().clone();
            let src_slot = call.screen_src.clone();
            let preview = call.screen_preview.clone();
            (call.pipeline.clone(), input, src, src_slot, preview)
        };
        let input =
            input.ok_or_else(|| anyhow!("screen sharing is not negotiated for this call"))?;
        if enabled {
            let source = match src {
                Some(src) => src,
                None => {
                    let source = self.create_screen_source()?;
                    attach_video_source(&pipeline, &input, &source)?;
                    *src_slot.write() = Some(source.clone());
                    source
                }
            };
            let _ = source.set_state(gst::State::Playing);
        } else {
            if let Some(src) = src {
                let _ = src.set_state(gst::State::Paused);
            }
            *preview.write() = None;
        }
        Ok(())
    }

    fn screen_preview_frame(&self, sid: &str) -> Option<CallPreview> {
        self.calls
            .lock()
            .get(sid)
            .and_then(|call| call.screen_preview.read().clone())
    }

    fn remote_screen_frame(&self, sid: &str) -> Option<CallPreview> {
        self.calls
            .lock()
            .get(sid)
            .and_then(|call| call.remote_screen.read().clone())
    }

    fn clear_remote_screen(&self, sid: &str) {
        if let Some(call) = self.calls.lock().get(sid) {
            *call.remote_screen.write() = None;
        }
    }

    async fn add_candidate(&self, sid: &str, candidate_xml: &str) -> Result<()> {
        let candidates = jingle_sdp::candidates_from_jingle(candidate_xml)?;
        let calls = self.calls.lock();
        let Some(call) = calls.get(sid) else {
            // The peer flush its local candidates before `session-initiate`
            // arrives. Keep them until `accept_offer`/`create_offer` inserts
            // the call instead of dropping the only reachable pair.
            tracing::debug!(
                "buffering {} ICE candidate(s) for unknown call {sid}",
                candidates.len()
            );
            self.pending_remote
                .lock()
                .entry(sid.to_string())
                .or_default()
                .extend(candidates);
            return Ok(());
        };
        let webrtc = call.webrtc.clone();
        drop(calls);
        for (mline, candidate) in candidates {
            webrtc.emit_by_name::<()>("add-ice-candidate", &[&mline, &candidate]);
        }
        Ok(())
    }

    async fn add_content_candidate(&self, sid: &str, key: &str, candidate_xml: &str) -> Result<()> {
        let webrtc = {
            let calls = self.calls.lock();
            calls
                .get(sid)
                .and_then(|call| call.receivers.get(key))
                .map(|receiver| receiver.webrtc.clone())
        };
        let Some(webrtc) = webrtc else {
            return self.add_candidate(sid, candidate_xml).await;
        };
        for (mline, candidate) in jingle_sdp::candidates_from_jingle(candidate_xml)? {
            webrtc.emit_by_name::<()>("add-ice-candidate", &[&mline, &candidate]);
        }
        Ok(())
    }

    fn remove_content(&self, sid: &str, key: &str) {
        let receiver = {
            let mut calls = self.calls.lock();
            let Some(call) = calls.get_mut(sid) else {
                return;
            };
            call.content_volumes.write().remove(key);
            call.content_levels.write().remove(key);
            call.receivers.remove(key)
        };
        if let Some(receiver) = receiver {
            let _ = receiver.pipeline.set_state(gst::State::Null);
        }
    }

    async fn set_muted(&self, sid: &str, muted: bool) -> Result<()> {
        let (mic, source, flag) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (
                call.mic.clone(),
                call.mic_src.clone(),
                call.mic_muted.clone(),
            )
        };
        flag.store(muted, Ordering::Relaxed);
        mic.set_property("mute", muted);
        if let Some(source) = source.read().as_ref() {
            apply_mute(source, muted);
        }
        Ok(())
    }

    async fn set_speaker_muted(&self, sid: &str, muted: bool) -> Result<()> {
        let (speaker, sink, flag, volumes) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (
                call.speaker.clone(),
                call.speaker_sink.clone(),
                call.speaker_muted.clone(),
                call.content_volumes.clone(),
            )
        };
        flag.store(muted, Ordering::Relaxed);
        if let Some(speaker) = speaker.read().as_ref() {
            speaker.set_property("mute", muted);
        }
        for volume in volumes.read().values() {
            volume.set_property("mute", muted);
        }
        if let Some(sink) = sink.read().as_ref() {
            apply_mute(sink, muted);
        }
        Ok(())
    }

    fn set_content_volume(&self, sid: &str, content: &str, gain: f64) {
        let calls = self.calls.lock();
        if let Some(call) = calls.get(sid) {
            if let Some(volume) = call.content_volumes.read().get(content) {
                volume.set_property("volume", gain.clamp(0.0, 2.0));
            }
        }
    }

    fn set_content_slides(&self, sid: &str, content: &str, slides: bool) {
        let calls = self.calls.lock();
        if let Some(call) = calls.get(sid) {
            let mut names = call.screen_contents.write();
            if slides {
                names.insert(content.to_string());
            } else {
                names.remove(content);
            }
        }
    }

    fn content_speaking(&self, sid: &str, content: &str) -> bool {
        let calls = self.calls.lock();
        calls.get(sid).is_some_and(|call| {
            call.content_levels
                .read()
                .get(content)
                .is_some_and(|last| last.elapsed() < Duration::from_millis(500))
        })
    }

    async fn set_camera(&self, sid: &str, enabled: bool) -> Result<()> {
        let (pipeline, input, src, src_slot, device_slot, preview, connected) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            let input = call.video_input.read().clone();
            let src = call.video_src.read().clone();
            let src_slot = call.video_src.clone();
            let device_slot = call.video_device.clone();
            let preview = call.preview.clone();
            let connected = call.connected.load(Ordering::Relaxed);
            (
                call.pipeline.clone(),
                input,
                src,
                src_slot,
                device_slot,
                preview,
                connected,
            )
        };
        let input = input.ok_or_else(|| anyhow!("video is not negotiated for this call"))?;
        let start = enabled && connected;
        if start {
            let source = match src {
                Some(src) => src,
                None => {
                    let (source, device) = self.create_camera_source()?;
                    attach_video_source(&pipeline, &input, &source)?;
                    *device_slot.write() = device;
                    *src_slot.write() = Some(source.clone());
                    source
                }
            };
            let _ = source.set_state(gst::State::Playing);
        } else if let Some(src) = src {
            let _ = src.set_state(gst::State::Paused);
        }
        if !start {
            *preview.write() = None;
        }
        Ok(())
    }

    fn preview_frame(&self, sid: &str) -> Option<CallPreview> {
        self.calls
            .lock()
            .get(sid)
            .and_then(|call| call.preview.read().clone())
    }

    fn remote_preview_frame(&self, sid: &str) -> Option<CallPreview> {
        self.calls
            .lock()
            .get(sid)
            .and_then(|call| call.remote_preview.read().clone())
    }

    fn camera_devices(&self) -> Vec<CameraDevice> {
        enumerate_camera_devices()
    }

    fn set_preferred_camera(&self, device_id: Option<&str>) {
        *self.preferred_camera.lock() = device_id.map(str::to_string);
    }

    fn start_story_camera(&self, device_id: Option<&str>) -> Result<()> {
        self.start_story_camera_impl(device_id)
    }

    fn stop_story_camera(&self) {
        self.stop_story_camera_impl();
    }

    fn story_camera_frame(&self) -> Option<CallPreview> {
        self.story_camera_frame_impl()
    }

    async fn capture_story_photo(&self, device_id: Option<&str>) -> Result<Vec<u8>> {
        self.stop_story_camera_impl();
        let preferred = device_id
            .map(str::to_string)
            .or_else(|| self.preferred_camera.lock().clone());
        tokio::task::spawn_blocking(move || capture_story_photo_blocking(preferred))
            .await
            .map_err(|error| anyhow!("camera capture task failed: {error}"))?
    }

    fn audio_devices(&self) -> Vec<AudioDevice> {
        audio_devices_with_id()
            .into_iter()
            .map(|(_, audio)| audio)
            .collect()
    }

    fn set_preferred_audio(&self, device_id: Option<&str>) {
        *self.preferred_audio.lock() = device_id.map(str::to_string);
    }

    fn active_camera_device(&self, sid: &str) -> Option<String> {
        self.calls
            .lock()
            .get(sid)
            .and_then(|call| call.video_device.read().clone())
    }

    async fn set_camera_device(&self, sid: &str, device_id: Option<&str>) -> Result<()> {
        *self.preferred_camera.lock() = device_id.map(str::to_string);
        let (pipeline, input, src, src_slot, device_slot, connected) = {
            let calls = self.calls.lock();
            let Some(call) = calls.get(sid) else {
                return Ok(());
            };
            let input = call.video_input.read().clone();
            let src = call.video_src.read().clone();
            let src_slot = call.video_src.clone();
            let device_slot = call.video_device.clone();
            let connected = call.connected.load(Ordering::Relaxed);
            (
                call.pipeline.clone(),
                input,
                src,
                src_slot,
                device_slot,
                connected,
            )
        };
        let (Some(input), Some(old)) = (input, src) else {
            return Ok(());
        };
        let playing = connected && matches!(old.current_state(), gst::State::Playing);
        let _ = old.set_state(gst::State::Null);
        let _ = pipeline.remove(&old);
        let (source, device) = self.create_camera_source()?;
        attach_video_source(&pipeline, &input, &source)?;
        *device_slot.write() = device;
        *src_slot.write() = Some(source.clone());
        let _ = source.set_state(if playing {
            gst::State::Playing
        } else {
            gst::State::Paused
        });
        Ok(())
    }

    async fn stop(&self, sid: &str) -> Result<()> {
        let call = { self.calls.lock().remove(sid) };
        self.pending_remote.lock().remove(sid);
        if let Some(mut call) = call {
            for (_, receiver) in call.receivers.drain() {
                let _ = receiver.pipeline.set_state(gst::State::Null);
            }
            let _ = call.pipeline.set_state(gst::State::Null);
            let no_more_calls = self.calls.lock().is_empty();
            if no_more_calls {
                *self.screen_node.lock() = None;
                *self.screen_session.lock() = None;
            }
        }
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<MediaEvent> {
        self.events.subscribe()
    }

    fn is_connected(&self, sid: &str) -> bool {
        self.calls
            .lock()
            .get(sid)
            .is_some_and(|call| call.connected.load(Ordering::Relaxed))
    }
}

fn make_preview_appsink(
    slot: Arc<RwLock<Option<CallPreview>>>,
    interval: Duration,
    quality: u8,
) -> Result<gst::Element> {
    let appsink = gst::ElementFactory::make("appsink").build()?;
    let appsink = appsink
        .downcast::<gst_app::AppSink>()
        .map_err(|_| anyhow!("appsink element has the wrong type"))?;
    appsink.set_property("max-buffers", 1u32);
    appsink.set_property("drop", true);
    appsink.set_property("sync", false);
    appsink.set_property("async", false);
    let sequence = Arc::new(AtomicU64::new(0));
    let mut last_frame = std::time::Instant::now();
    appsink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                if last_frame.elapsed() < interval {
                    return Ok(gst::FlowSuccess::Ok);
                }
                last_frame = std::time::Instant::now();
                if let Ok(sample) = sink.pull_sample() {
                    if let Some(caps) = sample.caps() {
                        if let Some(structure) = caps.structure(0) {
                            let width = structure.get::<i32>("width").unwrap_or(0);
                            let height = structure.get::<i32>("height").unwrap_or(0);
                            if width > 0 && height > 0 {
                                if let Some(buffer) = sample.buffer() {
                                    if let Ok(map) = buffer.map_readable() {
                                        if let Some(image) = image::RgbaImage::from_raw(
                                            u32::try_from(width).unwrap_or(0),
                                            u32::try_from(height).unwrap_or(0),
                                            map.as_slice().to_vec(),
                                        ) {
                                            let mut jpeg = Vec::new();
                                            if image::codecs::jpeg::JpegEncoder::new_with_quality(
                                                &mut jpeg, quality,
                                            )
                                            .encode_image(&image)
                                            .is_ok()
                                            {
                                                *slot.write() = Some(CallPreview {
                                                    width: u32::try_from(width).unwrap_or(0),
                                                    height: u32::try_from(height).unwrap_or(0),
                                                    jpeg: Arc::new(jpeg),
                                                    sequence: sequence
                                                        .fetch_add(1, Ordering::Relaxed),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    Ok(appsink.upcast())
}

fn camera_source_for(device_id: Option<&str>) -> Result<gst::Element> {
    if let Some(device_id) = device_id {
        for (device, camera) in camera_devices_with_id() {
            if camera.id == device_id {
                if let Ok(element) = device.create_element(None) {
                    return Ok(element);
                }
            }
        }
    }
    Ok(gst::ElementFactory::make("autovideosrc").build()?)
}

#[allow(clippy::needless_pass_by_value)]
fn capture_story_photo_blocking(device_id: Option<String>) -> Result<Vec<u8>> {
    let pipeline = gst::Pipeline::new();
    let source = camera_source_for(device_id.as_deref())?;
    let convert = gst::ElementFactory::make("videoconvert").build()?;
    let encoder = gst::ElementFactory::make("jpegenc")
        .property("quality", 90i32)
        .build()?;
    let appsink = gst::ElementFactory::make("appsink")
        .property("max-buffers", 1u32)
        .property("drop", true)
        .property("sync", false)
        .property("async", false)
        .property("emit-signals", false)
        .build()?;
    let appsink = appsink
        .downcast::<gst_app::AppSink>()
        .map_err(|_| anyhow!("appsink element has the wrong type"))?;
    let appsink_element = appsink.clone().upcast::<gst::Element>();
    pipeline.add_many([&source, &convert, &encoder, &appsink_element])?;
    gst::Element::link_many([&source, &convert, &encoder, &appsink_element])?;
    pipeline.set_state(gst::State::Playing)?;

    let started = std::time::Instant::now();
    let sample = loop {
        if let Some(sample) = appsink.try_pull_sample(gst::ClockTime::from_mseconds(100)) {
            break sample;
        }
        if started.elapsed() > std::time::Duration::from_secs(6) {
            let _ = pipeline.set_state(gst::State::Null);
            anyhow::bail!("timed out waiting for the camera");
        }
    };
    let buffer = sample
        .buffer()
        .ok_or_else(|| anyhow!("camera returned an empty frame"))?;
    let map = buffer
        .map_readable()
        .map_err(|_| anyhow!("camera frame is not readable"))?;
    let bytes = map.as_slice().to_vec();
    drop(map);
    let _ = pipeline.set_state(gst::State::Null);
    Ok(bytes)
}

/// Point a bundled Windows build at its own GStreamer runtime.
///
/// The installer and portable zip place the core DLLs next to `jabu.exe`, the
/// plugins under `lib\gstreamer-1.0` and the scanner under
/// `libexec\gstreamer-1.0`. The scanner is a child process, so it can only
/// resolve the core DLLs if the application directory is on `PATH`. Setting the
/// plugin and scanner variables also keeps a system-wide GStreamer installation
/// from overriding the bundled one.
#[cfg(target_os = "windows")]
fn configure_bundled_gstreamer() {
    use std::path::PathBuf;

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(exe_dir) = exe.parent() else {
        return;
    };

    let mut paths: Vec<PathBuf> = vec![exe_dir.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    if let Ok(joined) = std::env::join_paths(paths) {
        std::env::set_var("PATH", joined);
    }

    let plugin_dir = exe_dir.join("lib").join("gstreamer-1.0");
    if plugin_dir.is_dir() {
        std::env::set_var("GST_PLUGIN_SYSTEM_PATH_1_0", &plugin_dir);
    }

    let scanner = exe_dir
        .join("libexec")
        .join("gstreamer-1.0")
        .join("gst-plugin-scanner.exe");
    if scanner.is_file() {
        std::env::set_var("GST_PLUGIN_SCANNER_1_0", &scanner);
    }
}

/// Make the plugin directories this crate was built against available.
///
/// Nix keeps each plugin package in its own store path and relies on the shell
/// exporting `GST_PLUGIN_SYSTEM_PATH_1_0`. A process started outside that shell
/// (a desktop launcher, a stale terminal) then finds only the core plugins, and
/// `webrtcbin`, the Opus payloaders and the portal screen source all go missing.
/// The build records the value, so merge it into whatever the process inherited.
pub(crate) fn ensure_plugin_path() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        #[cfg(target_os = "windows")]
        configure_bundled_gstreamer();

        let Some(built) = option_env!("JABU_GST_PLUGIN_SYSTEM_PATH") else {
            return;
        };
        let mut paths: Vec<String> = std::env::var("GST_PLUGIN_SYSTEM_PATH_1_0")
            .unwrap_or_default()
            .split(':')
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect();
        for path in built.split(':').filter(|path| !path.is_empty()) {
            if !paths.iter().any(|existing| existing == path) {
                paths.push(path.to_string());
            }
        }
        std::env::set_var("GST_PLUGIN_SYSTEM_PATH_1_0", paths.join(":"));
    });
}

fn register_optional_plugins() {
    if gst::ElementFactory::find("pipewiresrc").is_some() {
        return;
    }
    for dir in [
        "/run/current-system/sw/lib/gstreamer-1.0",
        "/usr/lib/x86_64-linux-gnu/gstreamer-1.0",
        "/usr/lib64/gstreamer-1.0",
        "/usr/lib/gstreamer-1.0",
        "/usr/local/lib/gstreamer-1.0",
    ] {
        let path = std::path::Path::new(dir).join("libgstpipewire.so");
        if path.exists() {
            match gst::Plugin::load_file(&path) {
                Ok(_) => {
                    tracing::debug!("loaded pipewire GStreamer plugin from {}", path.display());
                    return;
                }
                Err(error) => {
                    tracing::debug!(
                        "could not load pipewire GStreamer plugin {}: {error}",
                        path.display()
                    );
                }
            }
        }
    }
}

fn camera_raw_caps() -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("width", 640i32)
        .field("height", 480i32)
        .field("framerate", gst::Fraction::new(30, 1))
        .build()
}

fn screen_raw_caps() -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .field("width", 1280i32)
        .field("height", 720i32)
        .field("framerate", gst::Fraction::new(15, 1))
        .build()
}

fn attach_video(
    pipeline: &gst::Pipeline,
    preview: &Arc<RwLock<Option<CallPreview>>>,
    sink_pad: gst::Pad,
) -> Result<gst::Element> {
    attach_video_branch(
        pipeline,
        preview,
        sink_pad,
        &camera_raw_caps(),
        (320, 240),
        CAMERA_PAYLOAD,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn attach_video_branch(
    pipeline: &gst::Pipeline,
    preview: &Arc<RwLock<Option<CallPreview>>>,
    sink_pad: gst::Pad,
    raw_caps: &gst::Caps,
    preview_size: (i32, i32),
    payload: i32,
    source: Option<&gst::Element>,
) -> Result<gst::Element> {
    let vconvert = gst::ElementFactory::make("videoconvert").build()?;
    let vscale = gst::ElementFactory::make("videoscale")
        .property("add-borders", true)
        .build()?;
    let vrate = gst::ElementFactory::make("videorate").build()?;
    let vfilter = gst::ElementFactory::make("capsfilter")
        .property("caps", raw_caps)
        .build()?;
    let tee = gst::ElementFactory::make("tee").build()?;
    let venc_queue = latest_queue()?;
    let vp8enc = gst::ElementFactory::make("vp8enc")
        .property("deadline", 1i64)
        .property("cpu-used", 8i32)
        .property("lag-in-frames", 0i32)
        .property("target-bitrate", 1_500_000i32)
        .property("keyframe-max-dist", 30i32)
        .property("threads", 4i32)
        .build()?;
    let vp8pay = gst::ElementFactory::make("rtpvp8pay").build()?;
    let vrtp = gst::Caps::builder("application/x-rtp")
        .field("media", "video")
        .field("encoding-name", "VP8")
        .field("payload", payload)
        .build();
    let vrtp_filter = gst::ElementFactory::make("capsfilter")
        .property("caps", &vrtp)
        .build()?;

    let preview_queue = latest_queue()?;
    let preview_convert = gst::ElementFactory::make("videoconvert").build()?;
    let preview_scale = gst::ElementFactory::make("videoscale").build()?;
    let preview_caps = gst::Caps::builder("video/x-raw")
        .field("format", "RGBA")
        .field("width", preview_size.0)
        .field("height", preview_size.1)
        .build();
    let preview_filter = gst::ElementFactory::make("capsfilter")
        .property("caps", &preview_caps)
        .build()?;
    let appsink_element = make_preview_appsink(preview.clone(), Duration::from_millis(100), 70)?;
    pipeline.add_many([
        &vconvert,
        &vscale,
        &vrate,
        &vfilter,
        &tee,
        &venc_queue,
        &vp8enc,
        &vp8pay,
        &vrtp_filter,
        &preview_queue,
        &preview_convert,
        &preview_scale,
        &preview_filter,
        &appsink_element,
    ])?;
    if let Some(source) = source {
        pipeline.add(source)?;
    }
    gst::Element::link_many([&vconvert, &vscale, &vrate, &vfilter])?;
    if let Some(source) = source {
        source
            .static_pad("src")
            .ok_or_else(|| anyhow!("screen source has no src pad"))?
            .link(
                &vconvert
                    .static_pad("sink")
                    .ok_or_else(|| anyhow!("videoconvert has no sink pad"))?,
            )?;
    }
    vfilter
        .static_pad("src")
        .ok_or_else(|| anyhow!("video capsfilter has no src pad"))?
        .link(
            &tee.static_pad("sink")
                .ok_or_else(|| anyhow!("tee has no sink pad"))?,
        )?;
    let _ = tee.link(&venc_queue);
    let _ = tee.link(&preview_queue);
    gst::Element::link_many([&venc_queue, &vp8enc, &vp8pay, &vrtp_filter])?;
    gst::Element::link_many([
        &preview_queue,
        &preview_convert,
        &preview_scale,
        &preview_filter,
    ])?;
    preview_filter
        .static_pad("src")
        .ok_or_else(|| anyhow!("preview capsfilter has no src pad"))?
        .link(
            &appsink_element
                .static_pad("sink")
                .ok_or_else(|| anyhow!("appsink has no sink pad"))?,
        )?;

    vrtp_filter
        .static_pad("src")
        .ok_or_else(|| anyhow!("video capsfilter has no src pad"))?
        .link(&sink_pad)?;

    let mut elements = vec![
        &appsink_element,
        &preview_filter,
        &preview_scale,
        &preview_convert,
        &preview_queue,
        &vrtp_filter,
        &vp8pay,
        &vp8enc,
        &venc_queue,
        &tee,
        &vfilter,
        &vrate,
        &vscale,
        &vconvert,
    ];
    if let Some(source) = source {
        elements.push(source);
    }
    for element in elements {
        let _ = element.sync_state_with_parent();
    }

    Ok(vconvert)
}

fn video_rtp_caps() -> gst::Caps {
    gst::Caps::builder("application/x-rtp")
        .field("media", "video")
        .field("encoding-name", "VP8")
        .field("payload", CAMERA_PAYLOAD)
        .field("clock-rate", 90000i32)
        .build()
}

fn screen_rtp_caps() -> gst::Caps {
    gst::Caps::builder("application/x-rtp")
        .field("media", "video")
        .field("encoding-name", "VP8")
        .field("payload", SCREEN_PAYLOAD)
        .field("clock-rate", 90000i32)
        .build()
}

fn add_screen_transceiver(webrtc: &gst::Element) -> Result<()> {
    let caps = screen_rtp_caps();
    let transceiver = webrtc.emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>(
        "add-transceiver",
        &[&gst_webrtc::WebRTCRTPTransceiverDirection::Sendonly, &caps],
    );
    transceiver
        .map(|_| ())
        .ok_or_else(|| anyhow!("webrtcbin refused to add a screen transceiver"))
}

fn add_video_transceiver(webrtc: &gst::Element) -> Result<()> {
    let caps = video_rtp_caps();
    let transceiver = webrtc.emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>(
        "add-transceiver",
        &[&gst_webrtc::WebRTCRTPTransceiverDirection::Sendrecv, &caps],
    );
    transceiver
        .map(|_| ())
        .ok_or_else(|| anyhow!("webrtcbin refused to add a video transceiver"))
}

fn attach_video_sender(
    pipeline: &gst::Pipeline,
    webrtc: &gst::Element,
    preview: &Arc<RwLock<Option<CallPreview>>>,
    video_input: &Arc<RwLock<Option<gst::Element>>>,
    sdp: &str,
) -> Result<()> {
    if video_input.read().is_some() {
        return Ok(());
    }
    let index = jingle_sdp::sdp_media_index(sdp, "video")
        .ok_or_else(|| anyhow!("SDP has no video media"))?;
    let transceiver = webrtc
        .emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>(
            "get-transceiver",
            &[&(index as i32)],
        )
        .ok_or_else(|| anyhow!("no transceiver for video m-line {index}"))?;
    transceiver.set_property(
        "direction",
        gst_webrtc::WebRTCRTPTransceiverDirection::Sendrecv,
    );
    let pad = sink_pad_for_mline(webrtc, index)
        .or_else(|| webrtc.request_pad_simple(&format!("sink_{index}")))
        .ok_or_else(|| anyhow!("webrtcbin has no sink pad for video m-line {index}"))?;
    let input = attach_video(pipeline, preview, pad)?;
    *video_input.write() = Some(input);
    Ok(())
}

fn attach_video_source(
    pipeline: &gst::Pipeline,
    input: &gst::Element,
    source: &gst::Element,
) -> Result<()> {
    pipeline.add(source)?;
    source
        .static_pad("src")
        .ok_or_else(|| anyhow!("camera source has no src pad"))?
        .link(
            &input
                .static_pad("sink")
                .ok_or_else(|| anyhow!("video input has no sink pad"))?,
        )?;
    let _ = source.sync_state_with_parent();
    Ok(())
}

fn enumerate_camera_devices() -> Vec<CameraDevice> {
    camera_devices_with_id()
        .into_iter()
        .map(|(_, camera)| camera)
        .collect()
}

pub(crate) fn audio_devices_with_id() -> Vec<(gst::Device, AudioDevice)> {
    let monitor = gst::DeviceMonitor::new();
    let _ = monitor.add_filter(Some("Audio/Source"), None);
    if monitor.start().is_err() {
        return Vec::new();
    }
    monitor
        .devices()
        .into_iter()
        .filter_map(|device| {
            let properties = device.properties()?;
            if properties
                .get::<String>("device.class")
                .ok()
                .is_some_and(|class| class == "monitor")
            {
                return None;
            }
            let name = device.display_name().to_string();
            if name.is_empty() {
                return None;
            }
            let id = ["node.name", "device.name", "device.string"]
                .iter()
                .find_map(|key| {
                    properties
                        .get::<String>(*key)
                        .ok()
                        .filter(|value| !value.is_empty())
                })
                .unwrap_or_else(|| name.clone());
            Some((device, AudioDevice { id, name }))
        })
        .collect()
}

fn camera_devices_with_id() -> Vec<(gst::Device, CameraDevice)> {
    let monitor = gst::DeviceMonitor::new();
    let _ = monitor.add_filter(Some("Video/Source"), None);
    if monitor.start().is_err() {
        return Vec::new();
    }
    monitor
        .devices()
        .into_iter()
        .filter_map(|device| {
            let name = device.display_name().to_string();
            if name.is_empty() {
                return None;
            }
            let id = camera_device_id(&device).unwrap_or_else(|| name.clone());
            Some((device, CameraDevice { id, name }))
        })
        .collect()
}

fn camera_device_id(device: &gst::Device) -> Option<String> {
    let properties = device.properties()?;
    for key in [
        "device.path",
        "api.v4l2.path",
        "device.name",
        "target-object",
    ] {
        if let Ok(value) = properties.get::<String>(key) {
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn sink_pad_for_mline(webrtc: &gst::Element, index: u32) -> Option<gst::Pad> {
    webrtc.pads().into_iter().find(|pad| {
        pad.direction() == gst::PadDirection::Sink
            && pad
                .property::<Option<gst_webrtc::WebRTCRTPTransceiver>>("transceiver")
                .is_some_and(|transceiver| transceiver.mlineindex() == index)
    })
}

pub(crate) fn apply_mute(element: &gst::Element, muted: bool) {
    if element.find_property("mute").is_some() {
        let _ = element.set_property("mute", muted);
    }
    if let Ok(bin) = element.clone().downcast::<gst::Bin>() {
        for child in bin.iterate_elements().into_iter().flatten() {
            apply_mute(&child, muted);
        }
    }
}

pub(crate) fn attach_child_mute(element: &gst::Element, muted: Arc<AtomicBool>) {
    if element.downcast_ref::<gst::Bin>().is_none() {
        return;
    }
    element.connect("child-added", false, move |values| {
        if let Ok(child) = values[1].get::<gst::Element>() {
            apply_mute(&child, muted.load(Ordering::Relaxed));
        }
        None
    });
}

pub(crate) fn latest_queue() -> Result<gst::Element> {
    let queue = gst::ElementFactory::make("queue").build()?;
    let _ = queue.set_property_from_str("leaky", "downstream");
    let _ = queue.set_property("max-size-buffers", 1u32);
    let _ = queue.set_property("max-size-time", 0u64);
    let _ = queue.set_property("max-size-bytes", 0u32);
    Ok(queue)
}

fn auto_sink(factory: &str, muted: &Arc<AtomicBool>) -> Result<gst::Element> {
    if std::env::var_os("JABU_AUDIO_TEST_FAKESINK").is_some() {
        return Ok(gst::ElementFactory::make("fakesink")
            .property("sync", false)
            .build()?);
    }
    let sink = gst::ElementFactory::make(factory).build()?;
    let muted = muted.clone();
    sink.connect("child-added", false, move |values| {
        if let Ok(child) = values[1].get::<gst::Element>() {
            if child.find_property("async").is_some() {
                let _ = child.set_property("async", false);
            }
            apply_mute(&child, muted.load(Ordering::Relaxed));
        }
        None
    });
    Ok(sink)
}

fn attach_receiver(
    pipeline: &gst::Pipeline,
    pad: &gst::Pad,
    media: &str,
    content_name: &str,
    preview_slot: &Arc<RwLock<Option<CallPreview>>>,
    speaker: &Arc<RwLock<Option<gst::Element>>>,
    speaker_sink: &Arc<RwLock<Option<gst::Element>>>,
    speaker_muted: &Arc<AtomicBool>,
    content_volumes: &Arc<RwLock<HashMap<String, gst::Element>>>,
    content_levels: &Arc<RwLock<HashMap<String, Instant>>>,
) -> Result<()> {
    let elements: Vec<gst::Element> = match media {
        "audio" => {
            let caps = gst::Caps::builder("audio/x-raw")
                .field("format", "F32LE")
                .build();
            let capsfilter = gst::ElementFactory::make("capsfilter")
                .property("caps", &caps)
                .build()?;
            let level_pad = capsfilter
                .static_pad("src")
                .ok_or_else(|| anyhow!("capsfilter has no src pad"))?;
            let levels = content_levels.clone();
            let level_name = content_name.to_string();
            level_pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
                if let Some(buffer) = info.buffer() {
                    if let Ok(map) = buffer.map_readable() {
                        let mut sum = 0.0f64;
                        let mut count = 0usize;
                        for chunk in map.as_slice().chunks_exact(4) {
                            let sample =
                                f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                            sum += f64::from(sample * sample);
                            count += 1;
                        }
                        if count > 0 && (sum / count as f64).sqrt() > 0.01 {
                            levels.write().insert(level_name.clone(), Instant::now());
                        }
                    }
                }
                gst::PadProbeReturn::Ok
            });

            let volume = gst::ElementFactory::make("volume")
                .name("speaker")
                .build()?;
            volume.set_property("mute", speaker_muted.load(Ordering::Relaxed));
            content_volumes
                .write()
                .insert(content_name.to_string(), volume.clone());
            *speaker.write() = Some(volume.clone());

            vec![
                gst::ElementFactory::make("queue").build()?,
                gst::ElementFactory::make("rtpopusdepay").build()?,
                gst::ElementFactory::make("opusdec").build()?,
                gst::ElementFactory::make("audioconvert").build()?,
                gst::ElementFactory::make("audioresample").build()?,
                capsfilter,
                volume,
                {
                    let sink = auto_sink("autoaudiosink", speaker_muted)?;
                    *speaker_sink.write() = Some(sink.clone());
                    sink
                },
            ]
        }
        "video" => {
            let caps = gst::Caps::builder("video/x-raw")
                .field("format", "RGBA")
                .field("width", 960i32)
                .field("height", 540i32)
                .build();
            vec![
                gst::ElementFactory::make("queue").build()?,
                gst::ElementFactory::make("rtpvp8depay").build()?,
                gst::ElementFactory::make("vp8dec").build()?,
                latest_queue()?,
                gst::ElementFactory::make("videoconvert").build()?,
                gst::ElementFactory::make("videoscale")
                    .property("add-borders", true)
                    .build()?,
                gst::ElementFactory::make("capsfilter")
                    .property("caps", &caps)
                    .build()?,
                make_preview_appsink(preview_slot.clone(), Duration::from_millis(66), 75)?,
            ]
        }
        _ => return Ok(()),
    };
    pipeline.add_many(elements.iter())?;
    gst::Element::link_many(elements.iter())?;
    for element in elements.iter().rev() {
        let _ = element.sync_state_with_parent();
    }
    let sink = elements[0]
        .static_pad("sink")
        .ok_or_else(|| anyhow!("receiver queue has no sink pad"))?;
    pad.link(&sink)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn attach_isolated_receiver(
    pipeline: &gst::Pipeline,
    webrtc: &gst::Element,
    key: &str,
    content_name: &str,
    slides: bool,
    remote_preview: &Arc<RwLock<Option<CallPreview>>>,
    remote_screen: &Arc<RwLock<Option<CallPreview>>>,
    speaker: &Arc<RwLock<Option<gst::Element>>>,
    speaker_sink: &Arc<RwLock<Option<gst::Element>>>,
    speaker_muted: &Arc<AtomicBool>,
    content_volumes: &Arc<RwLock<HashMap<String, gst::Element>>>,
    content_levels: &Arc<RwLock<HashMap<String, Instant>>>,
    events: &broadcast::Sender<MediaEvent>,
    sid: &str,
) {
    let pipeline_added = pipeline.clone();
    let key_added = key.to_string();
    let preview = remote_preview.clone();
    let screen = remote_screen.clone();
    let speaker_added = speaker.clone();
    let sink_added = speaker_sink.clone();
    let muted_added = speaker_muted.clone();
    let volumes_added = content_volumes.clone();
    let levels_added = content_levels.clone();
    webrtc.connect("pad-added", false, move |values| {
        let pad = values[1].get::<gst::Pad>().ok()?;
        if pad.direction() != gst::PadDirection::Src {
            return None;
        }
        let caps = pad.current_caps().or_else(|| Some(pad.query_caps(None)));
        let media = caps
            .as_ref()
            .and_then(|caps| caps.structure(0))
            .and_then(|structure| {
                (structure.name().as_str() == "application/x-rtp")
                    .then(|| structure.get::<String>("media").ok())
                    .flatten()
            });
        if let Some(media) = media.as_deref() {
            let slot = if media == "video" && slides {
                &screen
            } else {
                &preview
            };
            if let Err(error) = attach_receiver(
                &pipeline_added,
                &pad,
                media,
                &key_added,
                slot,
                &speaker_added,
                &sink_added,
                &muted_added,
                &volumes_added,
                &levels_added,
            ) {
                tracing::warn!("failed to attach isolated {media} receiver: {error}");
            }
        }
        None
    });

    let events_ice = events.clone();
    let sid_ice = sid.to_string();
    let key_ice = key.to_string();
    let name_ice = content_name.to_string();
    webrtc.connect("on-ice-candidate", false, move |values| {
        let mline = values[1].get::<u32>().ok()?;
        let candidate = values[2].get::<String>().ok()?;
        let (participant, name) = key_ice
            .split_once('|')
            .map_or((None, name_ice.as_str()), |(participant, name)| {
                (Some(participant), name)
            });
        if let Ok(jingle) =
            jingle_sdp::transport_info_for_participant(&sid_ice, name, participant, &candidate)
        {
            let _ = events_ice.send(MediaEvent::LocalCandidate {
                sid: sid_ice.clone(),
                mline,
                jingle,
            });
        }
        None
    });
}

fn connect_screen_sender(
    webrtc: &gst::Element,
    events: broadcast::Sender<MediaEvent>,
    sid: &str,
    content_names: Arc<RwLock<Vec<String>>>,
    pending_candidates: Arc<Mutex<Vec<(u32, String)>>>,
) {
    let sid_ice = sid.to_string();
    let content_ice = content_names;
    let pending_ice = pending_candidates;
    webrtc.connect("on-ice-candidate", false, move |values| {
        let mline = values[1].get::<u32>().ok()?;
        let candidate = values[2].get::<String>().ok()?;
        let name = content_ice.read().get(mline as usize).cloned();
        match name {
            Some(name) => {
                if let Ok(jingle) = jingle_sdp::transport_info(&sid_ice, &name, &candidate) {
                    let _ = events.send(MediaEvent::LocalCandidate {
                        sid: sid_ice.clone(),
                        mline,
                        jingle,
                    });
                }
            }
            None => pending_ice.lock().push((mline, candidate)),
        }
        None
    });
}

#[allow(clippy::too_many_arguments)]
fn connect_common(
    pipeline: &gst::Pipeline,
    webrtc: &gst::Element,
    events: &broadcast::Sender<MediaEvent>,
    sid: &str,
    content_names: Arc<RwLock<Vec<String>>>,
    remote_preview: Arc<RwLock<Option<CallPreview>>>,
    remote_screen: Arc<RwLock<Option<CallPreview>>>,
    speaker: Arc<RwLock<Option<gst::Element>>>,
    speaker_sink: Arc<RwLock<Option<gst::Element>>>,
    speaker_muted: Arc<AtomicBool>,
    content_volumes: Arc<RwLock<HashMap<String, gst::Element>>>,
    content_levels: Arc<RwLock<HashMap<String, Instant>>>,
    screen_contents: Arc<RwLock<HashSet<String>>>,
    pending_candidates: Arc<Mutex<Vec<(u32, String)>>>,
    connected: Arc<AtomicBool>,
    first_offer: Arc<AtomicBool>,
    offer_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
) {
    let pipeline = pipeline.clone();
    let content_names_added = content_names.clone();
    let screen_contents_added = screen_contents.clone();
    webrtc.connect("pad-added", false, move |values| {
        let pad = values[1].get::<gst::Pad>().ok()?;
        if pad.direction() != gst::PadDirection::Src {
            return None;
        }
        let caps = pad.current_caps().or_else(|| Some(pad.query_caps(None)));
        let media = caps
            .as_ref()
            .and_then(|caps| caps.structure(0))
            .and_then(|structure| {
                (structure.name().as_str() == "application/x-rtp")
                    .then(|| structure.get::<String>("media").ok())
                    .flatten()
            });
        if let Some(media) = media.as_deref() {
            let payload = caps
                .as_ref()
                .and_then(|caps| caps.structure(0))
                .and_then(|structure| structure.get::<i32>("payload").ok());
            let index = pad
                .name()
                .as_str()
                .strip_prefix("src_")
                .and_then(|value| value.parse::<usize>().ok());
            let content_name = index
                .and_then(|index| content_names_added.read().get(index).cloned())
                .unwrap_or_else(|| pad.name().to_string());
            let slides = screen_contents_added.read().contains(&content_name);
            let screen = media == "video" && (payload == Some(SCREEN_PAYLOAD) || slides);
            let slot = if screen {
                &remote_screen
            } else {
                &remote_preview
            };
            if let Err(error) = attach_receiver(
                &pipeline,
                &pad,
                media,
                &content_name,
                slot,
                &speaker,
                &speaker_sink,
                &speaker_muted,
                &content_volumes,
                &content_levels,
            ) {
                tracing::warn!("failed to attach {media} receiver: {error}");
            }
        }
        None
    });

    let events_ice = events.clone();
    let sid_ice = sid.to_string();
    let content_ice = content_names.clone();
    let pending_ice = pending_candidates.clone();
    webrtc.connect("on-ice-candidate", false, move |values| {
        let mline = values[1].get::<u32>().ok()?;
        let candidate = values[2].get::<String>().ok()?;
        tracing::debug!("call {sid_ice} local ICE candidate mline {mline}: {candidate}");
        let name = content_ice.read().get(mline as usize).cloned();
        match name {
            Some(name) => {
                if let Ok(jingle) = jingle_sdp::transport_info(&sid_ice, &name, &candidate) {
                    let _ = events_ice.send(MediaEvent::LocalCandidate {
                        sid: sid_ice.clone(),
                        mline,
                        jingle,
                    });
                }
            }
            None => pending_ice.lock().push((mline, candidate)),
        }
        None
    });

    let events_state = events.clone();
    let sid_state = sid.to_string();
    webrtc.connect_notify(Some("connection-state"), move |webrtc, _| {
        let state = webrtc.property::<gst_webrtc::WebRTCPeerConnectionState>("connection-state");
        tracing::debug!("call {sid_state} connection state: {state:?}");
        match state {
            gst_webrtc::WebRTCPeerConnectionState::Connected => {
                connected.store(true, Ordering::Relaxed);
                let _ = events_state.send(MediaEvent::Connected {
                    sid: sid_state.clone(),
                });
            }
            gst_webrtc::WebRTCPeerConnectionState::Disconnected
            | gst_webrtc::WebRTCPeerConnectionState::Failed
            | gst_webrtc::WebRTCPeerConnectionState::Closed => {
                connected.store(false, Ordering::Relaxed);
                let _ = events_state.send(MediaEvent::Disconnected {
                    sid: sid_state.clone(),
                });
            }
            _ => {}
        }
    });

    if let Some(offer_tx) = offer_tx {
        webrtc.connect("on-negotiation-needed", false, move |values| {
            if first_offer.swap(true, Ordering::SeqCst) {
                return None;
            }
            let webrtc = values[0].get::<gst::Element>().ok()?;
            let offer_tx = offer_tx.clone();
            let webrtc_for_offer = webrtc.clone();
            let promise = gst::Promise::with_change_func(move |reply| {
                if let Ok(Some(reply)) = reply {
                    if let Ok(offer) = reply.get::<gst_webrtc::WebRTCSessionDescription>("offer") {
                        webrtc_for_offer.emit_by_name::<()>(
                            "set-local-description",
                            &[&offer, &None::<gst::Promise>],
                        );
                        if let Ok(sdp) = offer.sdp().as_text() {
                            let _ = offer_tx.send(sdp);
                        }
                    }
                }
            });
            webrtc.emit_by_name::<()>("create-offer", &[&None::<gst::Structure>, &Some(promise)]);
            None
        });
    }
}

fn update_content_names(sdp: &str, content_names: &RwLock<Vec<String>>) {
    let mids = jingle_sdp::sdp_mids(sdp);
    if !mids.is_empty() {
        *content_names.write() = mids;
    }
}

fn flush_pending_candidates(
    events: &broadcast::Sender<MediaEvent>,
    sid: &str,
    content_names: &RwLock<Vec<String>>,
    pending: &Mutex<Vec<(u32, String)>>,
) {
    let drained: Vec<(u32, String)> = pending.lock().drain(..).collect();
    if drained.is_empty() {
        return;
    }
    let names = content_names.read();
    for (mline, candidate) in drained {
        let Some(name) = names.get(mline as usize) else {
            tracing::debug!("dropping candidate for unknown mline {mline} on call {sid}");
            continue;
        };
        if let Ok(jingle) = jingle_sdp::transport_info(sid, name, &candidate) {
            let _ = events.send(MediaEvent::LocalCandidate {
                sid: sid.to_string(),
                mline,
                jingle,
            });
        }
    }
}

pub(crate) async fn create_offer(webrtc: &gst::Element) -> Result<String> {
    let (tx, rx) = oneshot::channel();
    let webrtc_for_offer = webrtc.clone();
    let promise = gst::Promise::with_change_func(move |reply| {
        if let Ok(Some(reply)) = reply {
            if let Ok(offer) = reply.get::<gst_webrtc::WebRTCSessionDescription>("offer") {
                webrtc_for_offer
                    .emit_by_name::<()>("set-local-description", &[&offer, &None::<gst::Promise>]);
                if let Ok(sdp) = offer.sdp().as_text() {
                    let _ = tx.send(sdp);
                }
            }
        }
    });
    webrtc.emit_by_name::<()>("create-offer", &[&None::<gst::Structure>, &Some(promise)]);
    tokio::time::timeout(Duration::from_secs(15), rx)
        .await
        .map_err(|_| anyhow!("timed out waiting for the local offer"))?
        .map_err(|_| anyhow!("local offer channel closed"))
}

pub(crate) async fn create_answer(webrtc: &gst::Element) -> Result<String> {
    let (tx, rx) = oneshot::channel();
    let webrtc_for_answer = webrtc.clone();
    let promise = gst::Promise::with_change_func(move |reply| {
        if let Ok(Some(reply)) = reply {
            if let Ok(answer) = reply.get::<gst_webrtc::WebRTCSessionDescription>("answer") {
                webrtc_for_answer
                    .emit_by_name::<()>("set-local-description", &[&answer, &None::<gst::Promise>]);
                if let Ok(sdp) = answer.sdp().as_text() {
                    let _ = tx.send(sdp);
                }
            }
        }
    });
    webrtc.emit_by_name::<()>("create-answer", &[&None::<gst::Structure>, &Some(promise)]);
    tokio::time::timeout(Duration::from_secs(15), rx)
        .await
        .map_err(|_| anyhow!("timed out waiting for the local answer"))?
        .map_err(|_| anyhow!("local answer channel closed"))
}

pub(crate) async fn set_remote_description(
    webrtc: &gst::Element,
    sdp: &str,
    type_: gst_webrtc::WebRTCSDPType,
) -> Result<()> {
    let remote = sdp_description(sdp, type_)?;
    let (tx, rx) = oneshot::channel();
    let promise = gst::Promise::with_change_func(move |reply| {
        let _ = tx.send(reply.is_ok());
    });
    webrtc.emit_by_name::<()>("set-remote-description", &[&remote, &Some(promise)]);
    let applied = tokio::time::timeout(Duration::from_secs(15), rx)
        .await
        .map_err(|_| anyhow!("timed out applying the remote description"))?
        .map_err(|_| anyhow!("remote description channel closed"))?;
    if applied {
        Ok(())
    } else {
        Err(anyhow!("failed to apply the remote description"))
    }
}

fn sdp_description(
    sdp: &str,
    type_: gst_webrtc::WebRTCSDPType,
) -> Result<gst_webrtc::WebRTCSessionDescription> {
    let message = gst_sdp::SDPMessage::parse_buffer(sdp.as_bytes())
        .map_err(|error| anyhow!("invalid SDP: {error}"))?;
    Ok(gst_webrtc::WebRTCSessionDescription::new(type_, message))
}

fn with_credentials(url: &str, username: Option<&str>, credential: Option<&str>) -> String {
    let Some(username) = username else {
        return url.to_string();
    };
    let Some(credential) = credential else {
        return url.to_string();
    };
    // XEP-0215 returns `turn:host:port` without `//`, while webrtcbin wants a
    // URI. Accept both spellings.
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("turns://") {
        ("turns", rest)
    } else if let Some(rest) = url.strip_prefix("turn://") {
        ("turn", rest)
    } else if let Some(rest) = url.strip_prefix("turns:") {
        ("turns", rest)
    } else if let Some(rest) = url.strip_prefix("turn:") {
        ("turn", rest)
    } else {
        return url.to_string();
    };
    format!("{scheme}://{username}:{credential}@{rest}")
}

#[cfg(test)]
mod tests {
    use super::with_credentials;

    #[test]
    fn turn_credentials_accept_xep0215_and_uri_forms() {
        assert_eq!(
            with_credentials("turn:turn.example.org:3478", Some("u"), Some("p")),
            "turn://u:p@turn.example.org:3478"
        );
        assert_eq!(
            with_credentials(
                "turn:turn.example.org:3478?transport=udp",
                Some("u"),
                Some("p")
            ),
            "turn://u:p@turn.example.org:3478?transport=udp"
        );
        assert_eq!(
            with_credentials("turns:turn.example.org:5349", Some("u"), Some("p")),
            "turns://u:p@turn.example.org:5349"
        );
        assert_eq!(
            with_credentials("turn:turn.example.org:3478", None, None),
            "turn:turn.example.org:3478"
        );
    }
}
