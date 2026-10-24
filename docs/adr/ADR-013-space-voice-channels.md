# ADR-013: Space voice channels over a Galène SFU

## Status

Superseded by [ADR-014](ADR-014-sfu-conferences.md)

> This ADR records the interim direct-Galène WebSocket implementation. It is
> kept for history; the shipped client must not expose Galène connection
> settings. See ADR-014 for the XMPP SFU conference model.

## Context

Spaces need Discord-style voice channels. Full-mesh one-to-one Jingle does not
scale past a handful of participants: every client encodes and sends N-1
streams and decodes N-1 streams, and signaling grows quadratically. The target
is 10+ participants in one channel from the start.

The project already uses GStreamer `webrtcbin` for 1:1 calls, so keeping a
WebRTC media stack avoids a second implementation. Galène is a small SFU whose
protocol is documented and whose `auto-subgroups` feature creates groups on
demand, which maps naturally to dynamically created voice channels.

## Decision

- Model a voice channel as a space room of kind `voice`, persisted as a
  `<voice xmlns='urn:movim:spaces:voice:0'/>` extension in the XEP-0402
  bookmark item.
- Map a channel to the Galène group `<base group>/<space node>/<channel slug>`.
  The base group must enable `auto-subgroups`.
- Implement the Galène client in `crates/media` using the documented WebSocket
  protocol and GStreamer `webrtcbin`:
  - one up stream carrying the microphone,
  - one down stream per remote participant,
  - trickle ICE in both directions.
- Keep the SFU behind `xmpp_core::voice::VoicePort`; the UI never talks to
  Galène directly. A different SFU can replace the port later.
- Store the server URL, base group and optional group password in settings
  (`calls.galene_url`, `calls.galene_group`, `calls.galene_password`), and reuse
  the selected microphone (`calls.microphone`).
- Keep the local fixture (`galene/run.sh`) with a public group that has
  automatic subgroups and a wildcard user.

## Consequences

- A channel holds 10+ participants, with the server doing the fan-out.
- Operators must run Galène (or a compatible deployment) and enable automatic
  subgroups; without a server the Join button reports a configuration error.
- Media is decoded at the SFU, so voice channels are not end-to-end encrypted.
- The desktop client is implemented first; mobile and web still need a media
  backend.
- Microphone mute, active-speaker detection (from decoded audio energy) and
  per-participant playback volume are wired end to end. Global speaker routing
  remains a follow-up.
