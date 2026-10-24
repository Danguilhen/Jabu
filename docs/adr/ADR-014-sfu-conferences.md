# ADR-014: Space voice channels as XMPP SFU conferences

## Status

Accepted. Supersedes [ADR-013](ADR-013-space-voice-channels.md).

## Context

Spaces need Discord-style voice channels with 10+ participants. Full-mesh 1:1
Jingle does not scale, so the media has to go through an SFU. The interim
implementation (ADR-013) connected the desktop client directly to Galène's
WebSocket API. That worked for media, but it required every user to configure a
Galène URL, a base group and a shared group password, and it could not
interoperate with Movim.

Movim's SFU model is different:

- **Galener** is an XMPP component that wraps Galène. It joins a MUC as a
  virtual user, creates a Galène group through Galène's admin API and adds room
  members as Galène users. The client only speaks Jingle to the SFU.
- Discovery is via COIN (`urn:xmpp:coin:1`, `conference-info` in MUC presence)
  and the SFU user's capabilities.
- Galener's `content-add` uses a pre-standard extension:
  `<jingle-participant xmlns="{https://movim.eu}jingle_participant">` and XEP-0507
  `<category name="speaker|slides">`. Screen share is a second Jingle session.
- The ProtoXEP *Jingle Audio/Video Conferences*
  (`urn:xmpp:jingle:av-conferences:0`) instead uses one session per participant
  and `<origin entity="xmpp:…">`. Libervia has an experimental implementation.

Movim interoperability is a requirement, so the client must speak Galener's
dialect. Supporting the ProtoXEP as well keeps the door open to Libervia and
future clients.

## Decision

- A voice channel is a normal MUC room (XEP-0045) with a `voice` marker in its
  XEP-0402 bookmark. Text history remains available, like a Movim room.
- The client joins the MUC and discovers the SFU:
  - Galener: COIN `conference-info` in MUC presence and `urn:xmpp:coin:1`; call
    the SFU virtual user JID.
  - ProtoXEP: `urn:xmpp:jingle:av-conferences:0` and the
    `conference/audio-video` identity; call the room/service JID.
- Implement the **Galener dialect first**, then the ProtoXEP dialect. The
  parser accepts both; a session may use either origin element.
- Use one Jingle session per participant. The client sends its own audio (and
  later video/screen) with `senders="initiator"`; the SFU adds and removes
  remote contents with `content-add` / `content-remove`.
- Reuse the existing `webrtcbin` pipeline with dynamic receive contents. Map
  each content to a participant for tiles, active speaker and per-participant
  volume.
- Remove `calls.galene_url`, `calls.galene_group`, `calls.galene_password` and
  the direct Galène client mode from the shipped UI. A server without an SFU
  reports that voice channels are unavailable.
- Use Galener (PHP) server-side for the initial deployment and the in-repo
  testbed. A Rust Galener-compatible bridge is a follow-up; it will speak both
  dialects so PHP is not required long term.
- Add `dev/reference-testbed/` with Nix-based PHP 8.4, PostgreSQL, Galène, Movim and
  a Prosody `sfu.localhost` component. Capture real Galener stanzas as golden
  fixtures before implementing the client.

## Consequences

- Users configure nothing: join a voice room and press “Join voice”.
- Galener deployments interoperate with Movim; ProtoXEP SFUs interoperate with
  Libervia and other clients.
- The client grows a conference layer: discovery, COIN parsing, content
  add/remove, dual dialect origins, per-content media.
- Media is decoded at the SFU, so conferences are not end-to-end encrypted.
- PHP remains a server-side dependency until the Rust SFU bridge lands.
- Active speaker and per-participant volume move from per-stream pipelines to
  per-content probes on the shared session.
