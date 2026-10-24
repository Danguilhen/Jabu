//! Call media backends and Jingle/SDP conversion.

#[cfg(not(target_arch = "wasm32"))]
pub mod jingle_sdp;

pub use xmpp_core::jingle::MediaEvent;

#[cfg(not(target_arch = "wasm32"))]
pub mod conference_voice;
pub mod galene;
pub use conference_voice::ConferenceVoicePort;

#[cfg(not(target_arch = "wasm32"))]
pub mod gstreamer;

#[cfg(not(target_arch = "wasm32"))]
pub use galene::GaleneVoicePort;
#[cfg(not(target_arch = "wasm32"))]
pub use gstreamer::{GStreamerMediaPort, IceServer};

#[cfg(target_os = "linux")]
pub mod screencast;

#[cfg(not(target_os = "linux"))]
#[path = "screencast_stub.rs"]
pub mod screencast;
