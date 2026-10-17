//! Call media backends and Jingle/SDP conversion.

#[cfg(not(target_arch = "wasm32"))]
pub mod jingle_sdp;

pub use xmpp_core::jingle::MediaEvent;

#[cfg(not(target_arch = "wasm32"))]
pub mod gstreamer;

#[cfg(not(target_arch = "wasm32"))]
pub use gstreamer::{GStreamerMediaPort, IceServer};
