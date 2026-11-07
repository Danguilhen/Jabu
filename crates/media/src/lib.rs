//! Call media backends and Jingle/SDP conversion.

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub mod jingle_sdp;

pub use xmpp_core::jingle::MediaEvent;

#[cfg(not(target_arch = "wasm32"))]
pub mod conference_voice;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub mod galene;
pub use conference_voice::ConferenceVoicePort;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub mod gstreamer;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub use galene::GaleneVoicePort;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub use gstreamer::{GStreamerMediaPort, IceServer};

#[cfg(target_os = "linux")]
pub mod screencast;

#[cfg(not(target_os = "linux"))]
#[path = "screencast_stub.rs"]
pub mod screencast;
