//! GStreamer `webrtcbin` media backend (Linux desktop).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_sdp as gst_sdp;
use gstreamer_webrtc as gst_webrtc;
use parking_lot::Mutex;
use tokio::sync::{broadcast, oneshot};
use xmpp_core::jingle::{CallKind, CallPreview, MediaEvent, MediaPort};

use crate::jingle_sdp;

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
    connected: Arc<AtomicBool>,
    #[allow(dead_code)]
    content_name: Arc<parking_lot::RwLock<String>>,
    video_src: Option<gst::Element>,
    video_valve: Option<gst::Element>,
    preview: Arc<parking_lot::RwLock<Option<CallPreview>>>,
}

struct BuiltCall {
    pipeline: gst::Pipeline,
    webrtc: gst::Element,
    mic: gst::Element,
    recv_queue: gst::Element,
    recv_video: Option<gst::Element>,
    content_name: Arc<parking_lot::RwLock<String>>,
    video_src: Option<gst::Element>,
    video_valve: Option<gst::Element>,
    preview: Arc<parking_lot::RwLock<Option<CallPreview>>>,
}

/// GStreamer-backed media implementation.
pub struct GStreamerMediaPort {
    calls: Mutex<HashMap<String, GStreamerCall>>,
    events: broadcast::Sender<MediaEvent>,
    ice_servers: Mutex<Vec<IceServer>>,
}

impl GStreamerMediaPort {
    pub fn new(ice_servers: Vec<IceServer>) -> Result<Arc<Self>> {
        gst::init().map_err(|error| anyhow!("GStreamer init failed: {error}"))?;
        let (events, _) = broadcast::channel(64);
        Ok(Arc::new(Self {
            calls: Mutex::new(HashMap::new()),
            events,
            ice_servers: Mutex::new(ice_servers),
        }))
    }

    pub fn set_ice_servers(&self, servers: Vec<IceServer>) {
        *self.ice_servers.lock() = servers;
    }

    fn stun_turn(&self) -> (Option<String>, Option<String>) {
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

    fn build(&self, kind: CallKind) -> Result<BuiltCall> {
        let pipeline = gst::Pipeline::new();
        let webrtc = gst::ElementFactory::make("webrtcbin")
            .name("webrtc")
            .build()
            .context("missing webrtcbin (install gst-plugins-bad)")?;
        webrtc.set_property("bundle-policy", gst_webrtc::WebRTCBundlePolicy::MaxBundle);
        let (stun, turn) = self.stun_turn();
        if let Some(stun) = stun {
            webrtc.set_property("stun-server", stun);
        }
        if let Some(turn) = turn {
            webrtc.set_property("turn-server", turn);
        }
        pipeline.add(&webrtc)?;

        let src = gst::ElementFactory::make("autoaudiosrc").build()?;
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

        let recv_queue = gst::ElementFactory::make("queue")
            .name("recv_audio")
            .build()?;
        let depay = gst::ElementFactory::make("rtpopusdepay").build()?;
        let decoder = gst::ElementFactory::make("opusdec").build()?;
        let convert2 = gst::ElementFactory::make("audioconvert").build()?;
        let resample2 = gst::ElementFactory::make("audioresample").build()?;
        let output = gst::ElementFactory::make("autoaudiosink").build()?;
        pipeline.add_many([
            &recv_queue,
            &depay,
            &decoder,
            &convert2,
            &resample2,
            &output,
        ])?;
        gst::Element::link_many([
            &recv_queue,
            &depay,
            &decoder,
            &convert2,
            &resample2,
            &output,
        ])?;

        let mut video_src: Option<gst::Element> = None;
        let mut video_valve: Option<gst::Element> = None;
        let preview = Arc::new(parking_lot::RwLock::new(None::<CallPreview>));
        let recv_video = if kind == CallKind::Video {
            let vsrc = gst::ElementFactory::make("autovideosrc").build()?;
            let vconvert = gst::ElementFactory::make("videoconvert").build()?;
            let vcaps = gst::Caps::builder("video/x-raw")
                .field("width", 640i32)
                .field("height", 480i32)
                .field("framerate", gst::Fraction::new(30, 1))
                .build();
            let vfilter = gst::ElementFactory::make("capsfilter")
                .property("caps", &vcaps)
                .build()?;
            let tee = gst::ElementFactory::make("tee").build()?;
            let venc_queue = gst::ElementFactory::make("queue").build()?;
            let valve = gst::ElementFactory::make("valve").build()?;
            valve.set_property("drop", false);
            let vp8enc = gst::ElementFactory::make("vp8enc")
                .property("deadline", 1i64)
                .build()?;
            let vp8pay = gst::ElementFactory::make("rtpvp8pay").build()?;
            let vrtp = gst::Caps::builder("application/x-rtp")
                .field("media", "video")
                .field("encoding-name", "VP8")
                .field("payload", 97i32)
                .build();
            let vrtp_filter = gst::ElementFactory::make("capsfilter")
                .property("caps", &vrtp)
                .build()?;

            let preview_queue = gst::ElementFactory::make("queue").build()?;
            let preview_convert = gst::ElementFactory::make("videoconvert").build()?;
            let preview_scale = gst::ElementFactory::make("videoscale").build()?;
            let preview_caps = gst::Caps::builder("video/x-raw")
                .field("format", "RGBA")
                .field("width", 320i32)
                .field("height", 240i32)
                .build();
            let preview_filter = gst::ElementFactory::make("capsfilter")
                .property("caps", &preview_caps)
                .build()?;
            let appsink = gst::ElementFactory::make("appsink").build()?;
            let appsink = appsink
                .downcast::<gst_app::AppSink>()
                .map_err(|_| anyhow!("appsink element has the wrong type"))?;
            appsink.set_property("max-buffers", 1u32);
            appsink.set_property("drop", true);
            appsink.set_property("sync", false);
            {
                let slot = preview.clone();
                let sequence = Arc::new(AtomicU64::new(0));
                let mut last_frame = std::time::Instant::now();
                appsink.set_callbacks(
                    gst_app::AppSinkCallbacks::builder()
                        .new_sample(move |sink| {
                            if last_frame.elapsed() < Duration::from_millis(100) {
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
                                                            &mut jpeg, 70,
                                                        )
                                                        .encode_image(&image)
                                                        .is_ok()
                                                        {
                                                            *slot.write() = Some(CallPreview {
                                                                width: u32::try_from(width).unwrap_or(0),
                                                                height: u32::try_from(height).unwrap_or(0),
                                                                jpeg: Arc::new(jpeg),
                                                                sequence: sequence.fetch_add(
                                                                    1,
                                                                    Ordering::Relaxed,
                                                                ),
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
            }
            let appsink_element: gst::Element = appsink.clone().upcast();
            pipeline.add_many([
                &vsrc,
                &vconvert,
                &vfilter,
                &tee,
                &venc_queue,
                &valve,
                &vp8enc,
                &vp8pay,
                &vrtp_filter,
                &preview_queue,
                &preview_convert,
                &preview_scale,
                &preview_filter,
                &appsink_element,
            ])?;
            vfilter
                .static_pad("src")
                .ok_or_else(|| anyhow!("video capsfilter has no src pad"))?
                .link(
                    &tee.static_pad("sink")
                        .ok_or_else(|| anyhow!("tee has no sink pad"))?,
                )?;
            let _ = tee.link(&venc_queue);
            let _ = tee.link(&preview_queue);
            gst::Element::link_many([&venc_queue, &valve, &vp8enc, &vp8pay, &vrtp_filter])?;
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
                    &appsink
                        .static_pad("sink")
                        .ok_or_else(|| anyhow!("appsink has no sink pad"))?,
                )?;

            let vsink = webrtc
                .request_pad_simple("sink_1")
                .or_else(|| webrtc.request_pad_simple("sink_%u"))
                .ok_or_else(|| anyhow!("webrtcbin has no video sink pad"))?;
            vrtp_filter
                .static_pad("src")
                .ok_or_else(|| anyhow!("video capsfilter has no src pad"))?
                .link(&vsink)?;

            let vqueue = gst::ElementFactory::make("queue")
                .name("recv_video")
                .build()?;
            let vdepay = gst::ElementFactory::make("rtpvp8depay").build()?;
            let vdec = gst::ElementFactory::make("vp8dec").build()?;
            let vconv2 = gst::ElementFactory::make("videoconvert").build()?;
            let vout = gst::ElementFactory::make("autovideosink").build()?;
            pipeline.add_many([&vqueue, &vdepay, &vdec, &vconv2, &vout])?;
            gst::Element::link_many([&vqueue, &vdepay, &vdec, &vconv2, &vout])?;
            video_src = Some(vsrc);
            video_valve = Some(valve);
            Some(vqueue)
        } else {
            None
        };
        let content_name = Arc::new(parking_lot::RwLock::new(kind.as_str().to_string()));
        Ok(BuiltCall {
            pipeline,
            webrtc,
            mic,
            recv_queue,
            recv_video,
            content_name,
            video_src,
            video_valve,
            preview,
        })
    }
}

#[async_trait]
impl MediaPort for GStreamerMediaPort {
    async fn create_offer(&self, sid: &str, kind: CallKind) -> Result<String> {
        let BuiltCall {
            pipeline,
            webrtc,
            mic,
            recv_queue,
            recv_video,
            content_name,
            video_src,
            video_valve,
            preview,
        } = self.build(kind)?;
        let connected = Arc::new(AtomicBool::new(false));
        let (offer_tx, mut offer_rx) = tokio::sync::mpsc::unbounded_channel();
        connect_common(
            &webrtc,
            &recv_queue,
            recv_video.clone(),
            &self.events,
            sid,
            content_name.clone(),
            connected.clone(),
            Some(offer_tx),
        );

        pipeline.set_state(gst::State::Playing)?;
        let sdp = tokio::time::timeout(Duration::from_secs(15), offer_rx.recv())
            .await
            .map_err(|_| anyhow!("timed out waiting for the local offer"))?
            .ok_or_else(|| anyhow!("local offer channel closed"))?;
        let jingle =
            jingle_sdp::sdp_to_jingle(&sdp, xmpp_parsers::jingle::Action::SessionInitiate, sid)?;
        self.calls.lock().insert(
            sid.to_string(),
            GStreamerCall {
                pipeline,
                webrtc,
                mic,
                connected,
                content_name,
                video_src,
                video_valve,
                preview,
            },
        );
        Ok(jingle)
    }

    async fn accept_offer(&self, sid: &str, kind: CallKind, offer: &str) -> Result<String> {
        let BuiltCall {
            pipeline,
            webrtc,
            mic,
            recv_queue,
            recv_video,
            content_name,
            video_src,
            video_valve,
            preview,
        } = self.build(kind)?;
        if let Ok(name) = jingle_sdp::first_content_name(offer) {
            *content_name.write() = name;
        }
        let connected = Arc::new(AtomicBool::new(false));
        connect_common(
            &webrtc,
            &recv_queue,
            recv_video.clone(),
            &self.events,
            sid,
            content_name.clone(),
            connected.clone(),
            None,
        );

        pipeline.set_state(gst::State::Playing)?;
        let remote_sdp = jingle_sdp::jingle_to_sdp(offer)?;
        let remote = sdp_description(&remote_sdp, gst_webrtc::WebRTCSDPType::Offer)?;
        let (answer_tx, answer_rx) = oneshot::channel();
        let webrtc_for_answer = webrtc.clone();
        let promise = gst::Promise::with_change_func(move |reply| {
            if reply.is_err() {
                return;
            }
            let webrtc = webrtc_for_answer;
            let webrtc_inner = webrtc.clone();
            let answer_promise = gst::Promise::with_change_func(move |reply| {
                if let Ok(Some(reply)) = reply {
                    if let Ok(answer) = reply.get::<gst_webrtc::WebRTCSessionDescription>("answer")
                    {
                        webrtc_inner.emit_by_name::<()>(
                            "set-local-description",
                            &[&answer, &None::<gst::Promise>],
                        );
                        if let Ok(sdp) = answer.sdp().as_text() {
                            let _ = answer_tx.send(sdp);
                        }
                    }
                }
            });
            webrtc.emit_by_name::<()>(
                "create-answer",
                &[&None::<gst::Structure>, &Some(answer_promise)],
            );
        });
        webrtc.emit_by_name::<()>("set-remote-description", &[&remote, &promise]);

        let sdp = tokio::time::timeout(Duration::from_secs(15), answer_rx)
            .await
            .map_err(|_| anyhow!("timed out waiting for the local answer"))?
            .map_err(|_| anyhow!("local answer channel closed"))?;
        let jingle =
            jingle_sdp::sdp_to_jingle(&sdp, xmpp_parsers::jingle::Action::SessionAccept, sid)?;
        self.calls.lock().insert(
            sid.to_string(),
            GStreamerCall {
                pipeline,
                webrtc,
                mic,
                connected,
                content_name,
                video_src,
                video_valve,
                preview,
            },
        );
        Ok(jingle)
    }

    async fn apply_answer(&self, sid: &str, answer: &str) -> Result<()> {
        let (webrtc, pipeline) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (call.webrtc.clone(), call.pipeline.clone())
        };
        let remote_sdp = jingle_sdp::jingle_to_sdp(answer)?;
        let remote = sdp_description(&remote_sdp, gst_webrtc::WebRTCSDPType::Answer)?;
        webrtc.emit_by_name::<()>("set-remote-description", &[&remote, &None::<gst::Promise>]);
        pipeline.set_state(gst::State::Playing)?;
        Ok(())
    }

    async fn add_candidate(&self, sid: &str, candidate_xml: &str) -> Result<()> {
        let webrtc = {
            let calls = self.calls.lock();
            calls
                .get(sid)
                .map(|call| call.webrtc.clone())
                .ok_or_else(|| anyhow!("unknown call {sid}"))?
        };
        for (mline, candidate) in jingle_sdp::candidates_from_jingle(candidate_xml)? {
            webrtc.emit_by_name::<()>("add-ice-candidate", &[&mline, &candidate]);
        }
        Ok(())
    }

    async fn set_muted(&self, sid: &str, muted: bool) -> Result<()> {
        let mic = {
            let calls = self.calls.lock();
            calls
                .get(sid)
                .map(|call| call.mic.clone())
                .ok_or_else(|| anyhow!("unknown call {sid}"))?
        };
        mic.set_property("mute", muted);
        Ok(())
    }

    async fn set_camera(&self, sid: &str, enabled: bool) -> Result<()> {
        let (valve, src, preview) = {
            let calls = self.calls.lock();
            let call = calls
                .get(sid)
                .ok_or_else(|| anyhow!("unknown call {sid}"))?;
            (
                call.video_valve.clone(),
                call.video_src.clone(),
                call.preview.clone(),
            )
        };
        let valve = valve.ok_or_else(|| anyhow!("video is not negotiated for this call"))?;
        valve.set_property("drop", !enabled);
        if let Some(src) = src {
            let _ = src.set_state(if enabled {
                gst::State::Playing
            } else {
                gst::State::Paused
            });
        }
        if !enabled {
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

    async fn stop(&self, sid: &str) -> Result<()> {
        if let Some(call) = self.calls.lock().remove(sid) {
            let _ = call.pipeline.set_state(gst::State::Null);
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

#[allow(clippy::too_many_arguments)]
fn connect_common(
    webrtc: &gst::Element,
    recv_queue: &gst::Element,
    recv_video: Option<gst::Element>,
    events: &broadcast::Sender<MediaEvent>,
    sid: &str,
    content_name: Arc<parking_lot::RwLock<String>>,
    connected: Arc<AtomicBool>,
    offer_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
) {
    let recv_queue = recv_queue.clone();
    let recv_video = recv_video.clone();
    webrtc.connect("pad-added", false, move |values| {
        let pad = values[1].get::<gst::Pad>().ok()?;
        let caps = pad.current_caps().or_else(|| Some(pad.query_caps(None)));
        let media = caps
            .as_ref()
            .and_then(|caps| caps.structure(0))
            .and_then(|structure| {
                (structure.name().as_str() == "application/x-rtp")
                    .then(|| structure.get::<String>("media").ok())
                    .flatten()
            });
        match media.as_deref() {
            Some("audio") => {
                if let Some(sink) = recv_queue.static_pad("sink") {
                    let _ = pad.link(&sink);
                }
            }
            Some("video") => {
                if let Some(queue) = recv_video.as_ref() {
                    if let Some(sink) = queue.static_pad("sink") {
                        let _ = pad.link(&sink);
                    }
                }
            }
            _ => {}
        }
        None
    });

    let events_ice = events.clone();
    let sid_ice = sid.to_string();
    let content_ice = content_name.clone();
    webrtc.connect("on-ice-candidate", false, move |values| {
        let mline = values[1].get::<u32>().ok()?;
        let candidate = values[2].get::<String>().ok()?;
        let content_name = content_ice.read().clone();
        if let Ok(jingle) = jingle_sdp::transport_info(&sid_ice, &content_name, &candidate) {
            let _ = events_ice.send(MediaEvent::LocalCandidate {
                sid: sid_ice.clone(),
                mline,
                jingle,
            });
        }
        None
    });

    let events_state = events.clone();
    let sid_state = sid.to_string();
    webrtc.connect_notify(Some("connection-state"), move |webrtc, _| {
        let state = webrtc.property::<gst_webrtc::WebRTCPeerConnectionState>("connection-state");
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
        let content_offer = content_name.clone();
        webrtc.connect("on-negotiation-needed", false, move |values| {
            let webrtc = values[0].get::<gst::Element>().ok()?;
            let offer_tx = offer_tx.clone();
            let content_offer = content_offer.clone();
            let webrtc_for_offer = webrtc.clone();
            let promise = gst::Promise::with_change_func(move |reply| {
                if let Ok(Some(reply)) = reply {
                    if let Ok(offer) = reply.get::<gst_webrtc::WebRTCSessionDescription>("offer") {
                        webrtc_for_offer.emit_by_name::<()>(
                            "set-local-description",
                            &[&offer, &None::<gst::Promise>],
                        );
                        if let Ok(sdp) = offer.sdp().as_text() {
                            if let Ok(mid) = jingle_sdp::sdp_first_mid(&sdp) {
                                *content_offer.write() = mid;
                            }
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

fn sdp_description(
    sdp: &str,
    type_: gst_webrtc::WebRTCSDPType,
) -> Result<gst_webrtc::WebRTCSessionDescription> {
    let message = gst_sdp::SDPMessage::parse_buffer(sdp.as_bytes())
        .map_err(|error| anyhow!("invalid SDP: {error}"))?;
    Ok(gst_webrtc::WebRTCSessionDescription::new(type_, message))
}

fn with_credentials(url: &str, username: Option<&str>, credential: Option<&str>) -> String {
    match (username, credential) {
        (Some(username), Some(credential)) => {
            if let Some(rest) = url.strip_prefix("turn://") {
                format!("turn://{username}:{credential}@{rest}")
            } else if let Some(rest) = url.strip_prefix("turns://") {
                format!("turns://{username}:{credential}@{rest}")
            } else {
                url.to_string()
            }
        }
        _ => url.to_string(),
    }
}
