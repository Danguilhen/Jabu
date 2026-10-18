# ADR-010: Call Media Backend

## Status

Accepted for desktop; Android remains pending.

## Context

Calls require both a Jingle control plane and a media plane. The control plane must be
pure Rust and interoperable with Movim, Dino and Galener. The media plane must capture,
encode, transport and render audio/video on Linux desktop and Android. Dioxus uses a
WebView on both platforms.

## Decision

Introduce a `CallMedia` abstraction and decide the backend after an early spike:

1. WebView WebRTC via a Rust/JavaScript bridge (primary candidate for Android, and for
   desktop where WebKitGTK provides working `getUserMedia`/`RTCPeerConnection`).
2. Native `webrtc-rs` plus GStreamer/PipeWire capture and a rendering surface if desktop
   WebKit is insufficient.
3. A hybrid of the two if required.

The Jingle/SDP control plane is identical for all backends. No SFU server is implemented;
the client interoperates with Galener and Galene-backed deployments.

## Rationale

- WebView WebRTC gives platform media I/O and device handling for free on Android.
- Desktop WebKit WebRTC support varies by distribution and must be verified.
- A common control plane keeps interop and protocol tests independent of media backend.

## Consequences

- No call UI is built before an audio path works on desktop and Android.
- Android needs camera/microphone permissions, a foreground service, audio focus and
  lifecycle handling.
- Desktop needs screen capture through PipeWire/portal and a fallback strategy if WebKit
  lacks WebRTC.

## Implementation notes

The desktop path uses GStreamer `webrtcbin` behind the `MediaPort` trait in `crates/media`:

- Audio is Opus/RTP; video is VP8/RTP with `vp8enc` and `autovideosrc`.
- The local camera preview is a `tee` branch into `appsink`; JPEG frames are shown in the
  Dioxus UI (about 10 fps at 320x240).
- The camera source is created lazily and started only once the call is connected.
  Turning the camera off pauses the source without renegotiating.
- An audio call can be upgraded to video with a Jingle `content-add`; the SDP
  renegotiation is merged locally and the peer's `content-accept` is applied as an answer.
- Local camera devices are enumerated with `GstDeviceMonitor`. The preferred device is
  stored in the UI settings and can be switched during a call.
- Receive branches are created on `pad-added` so the pipeline can reach `PLAYING` before
  RTP arrives.

Android still needs a WebRTC/GStreamer backend plus runtime camera and microphone
permissions; the platform crate currently only exposes notification permission stubs.
