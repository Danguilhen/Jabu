# ADR-010: Call Media Backend

## Status

Proposed

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
