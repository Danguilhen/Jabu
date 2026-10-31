# Jabu — parity plan

Status: living plan, reconciled 2026-11-06.
Scope: full user-facing Movim parity, with Linux desktop and Android as the
release-gating platforms and the web as a following target.
Reference: Movim (AGPLv3) is a behaviour/spec reference only. This is a
clean-room implementation.

Current implementation state is tracked in **[docs/STATUS.md](STATUS.md)**.
This document describes direction and remaining work, not what already works.

## 1. Locked decisions

1. **Full parity is the goal**, including public pages, instance
   administration, moderation, reporting, share links, Spaces, Stories, group
   video and the web.
2. **Linux desktop is the current release target.** Android follows once a
   native media backend and platform integration exist. The web remains a
   target but is blocked on a wasm-safe protocol/crypto path and a WebSocket
   transport.
3. **Voice channels use the XMPP SFU model**, never the Galène WebSocket API.
   The client joins the MUC, discovers the SFU and speaks Jingle. Galener is the
   first dialect for Movim interoperability; the ProtoXEP *Jingle Audio/Video
   Conferences* dialect follows for Libervia and other clients. Galener (PHP)
   is the server-side reference until a Rust bridge exists.
4. **Storage is SQLite + FTS5.** Redb is retained only for a one-release
   settings import. Credentials and the OMEMO at-rest key use the OS keyring on
   desktop, with an explicit plaintext fallback when no keyring is available.
5. **Dioxus 0.7.10 is the UI line.** Never mix Dioxus crate versions.
6. **Public pages and administration require a Host surface.** A pure client
   cannot serve public URLs. An optional headless Host/Gateway crate is planned;
   the desktop app can run it and Android can connect to it.
7. **UI-to-core boundary is a target, not a fact.** ADR-008 describes an
   `AppService`/`AccountManager` boundary. Today the UI calls `XmppClient`
   directly and owns the connection lifecycle. The refactor is tracked here and
   must not be assumed by new code until it lands.

## 2. Target architecture

```
crates/
  xmpp-core/    Protocol engine only. No Dioxus types.
                connection, typed IQ router, roster, chat, MUC, PubSub/PEP,
                OMEMO, Jingle, Spaces, Stories, calls signalling.

  storage/      SQLite + FTS5 system of record.
                Migrations, account scoping, message/post/space/story indexes,
                search, moderation/report tables, call history, drafts.

  media/        CallMedia and VoicePort backends.
                Desktop GStreamer webrtcbin today; Android backend to be chosen;
                WebView WebRTC is an option where platform support is reliable.

  host/         Planned. Optional headless Host/Gateway.
                Axum HTTP server, public pages, Atom/RSS, share links, report
                intake, moderation queue, admin console, audit log.

  platform/     Desktop and Android backends behind cfg.
                notifications, keyring/Keystore, files, permissions,
                foreground service, push, intents, screen capture.

  ui/           Dioxus presentation. Reads app state, sends commands, renders
                events.

  app/          Composition root, runtime, account manager, routing, Host mode.
```

### Core rules

- `xmpp-core` compiles and runs headless, with no Dioxus dependency.
- A typed IQ router correlates requests with results/errors and applies
  timeouts. No fire-and-forget IQ where a response matters.
- Every persistent write is account-scoped and transactional.
- Experimental XEPs are capability-gated.
- The target service boundary is `AppService` + `AccountManager`; until it is
  implemented, document the direct-client path instead of pretending otherwise.

### Account model

- Target: one `AccountSession` per configured JID, each with its own connection,
  OMEMO state, storage namespace and event stream, all online at once.
- Current: one active connection. Switching accounts reconnects. This is the
  main gap between the current app and Movim's multi-account model.

## 3. Remaining milestones

Milestones below list **remaining work only**. Completed items are recorded in
STATUS.md. Each milestone ends with a demo against a real server and an updated
STATUS.md.

### Milestone 0 — Foundation and release engineering

**Current state:** protocol core, SQLite storage with forward-only migrations,
reconnect/resumption and test infrastructure are in place. The milestone is not
closed because CI and the secret-storage decision are not finished.

Remaining:

- Make every CI job honest and green:
  - the `check`/`test` jobs now install the required GTK/WebKit/GStreamer
    packages; re-run the workflow before closing this;
  - add a real wasm job only once `xmpp-core` compiles for wasm;
  - keep the Android job build-only until an emulator smoke test exists;
  - run the Prosody and media suites in jobs that actually start their services.
- Verify the keyring path on GNOME/Secret Service and KDE/KWallet, and add
  Android Keystore support. Credentials and OMEMO state already use the desktop
  keyring with a documented SQLite fallback.
- Decide the wasm transport strategy (WebSocket via `tokio-xmpp` or a separate
  wasm transport) and the wasm crypto/TLS replacement for `aws-lc-sys`.
- Keep `cargo fmt`, `cargo clippy` and hermetic tests green on the pinned
  toolchain.

Exit criteria: a clean checkout builds, tests and packages without undeclared
system dependencies; secrets are not plaintext; migrates an old database.

### Milestone 1 — Accounts, identity, presence and contacts

**Current state:** identity, presence, vCard/vCard4, PEP activity, avatars,
roster, blocking and gateways work on desktop. Android and simultaneous
multi-account sessions are open.

Remaining:

- Simultaneous multi-account sessions (`AccountSession` per JID).
- Per-account settings, notification policy and storage namespace.
- Android login, permission and notification flow.
- Contact suggestions and richer group management if Movim parity requires it.

Exit criteria: a user can manage several identities, accounts, presence and
contacts on desktop and Android without reconnecting other accounts.

### Milestone 2 — Complete chat and rooms

**Current state:** 1:1 and MUC chat are substantially complete on desktop,
including OMEMO 1:1, uploads, reactions, edits, retractions, replies, mentions,
search, drafts, pins, XEP-0410 room self-ping, XEP-0425 moderator retraction,
XEP-0428 fallback handling, XEP-0469 bookmark pinning, XEP-0201 threads,
XEP-0433 room search and on-demand link previews.

Remaining:

- MUC OMEMO: sender devices, trust, multi-device, key rotation.
- Full RSM for remaining PubSub collections (spaces and PEP lists).
- GIF/Tenor integration (deferred until a provider decision).
- Attachment cache, thumbnails and download management.
- Android message pipeline and notification integration.

Exit criteria: 1:1 and MUC feature parity on desktop, plus a usable Android
chat path with encryption and media.

### Milestone 3 — Social: feed, communities, Spaces, Stories, Host

**Current state:** Spaces discovery/create/join/rooms/members and a microblog
subset work. Stories, communities, explore, comments, likes and public pages do
not.

Remaining:

- Complete XEP-0277: Markdown/rich content, comments and likes, edit, drafts
  and per-post notifications.
- Communities and explore: subscriptions, tags, server discovery, suggestions.
- XEP-0472/0496/0497/0499 where the server supports them.
- XEP-0462 type filtering and XEP-0465 public subscriptions.
- Stories (XEP-0501): publish image/video with 24 h expiry, ring, viewed state,
  viewer list, private reactions/replies, host pages and share links.
- Host crate: public routes, OpenGraph/Twitter cards, Atom/RSS, sitemap,
  robots, ETag/Last-Modified, access control and rate limiting.
- Share links and `jabu://` deep links.

Exit criteria: feed, communities, Spaces and Stories work against a
Movim-compatible service; a Host instance serves public pages and share links.

### Milestone 4 — Calls and voice

**Current state:** desktop 1:1 audio/video and SFU voice channels work,
including Galener and ProtoXEP signalling, active speaker, per-participant
volume, mute and screen share. Android media and group camera video are open.

Remaining:

- Validate the committed screen-share/portal fixes against the testbed on X11
  and Wayland.
- Group camera video in SFU conferences.
- Android media backend (WebView WebRTC or native), permissions, audio focus,
  foreground service and lifecycle handling.
- Movim ↔ Rust interop through Galener, and Libervia ↔ Rust through ProtoXEP.
- 10+ participant load, network switch, screen-off and process-death tests.
- Replace the manual `calls.sfu` override with automatic discovery where the
  server exposes the component; keep the override only as a fallback.
- Rust SFU bridge (no PHP): XEP-0114 component, MUC presence/COIN, Galène admin
  API and WebSocket translation, emitting both dialects.
- Remove the interim `GaleneVoicePort` library path once the Rust bridge or a
  supported SFU makes it unnecessary.

Exit criteria: desktop ↔ Android 1:1 calls; 10+ participant voice channels with
screen share and group video; verified Movim and ProtoXEP interop.

### Milestone 5 — Public host, moderation, reporting, administration

**Current state:** not started. ADR-009 defines the target.

Remaining:

- `crates/host` Axum binary and deployment configuration.
- Public routes: `/`, `/blog/{jid}`, `/post/...`, `/community/...`,
  `/space/...`, `/story/...`, `/contact/...`, `/about`, `/report`, `/admin`.
- Public content cache populated from XMPP/PubSub; no PHP dependency.
- Report intake, moderation queue, actions, reporter notification, audit log.
- Host admin authentication, roles, instance settings, branding, federation/
  cache policy, retention, storage and backups.
- In-app report actions for users, messages, posts, rooms, spaces and Stories.
- Desktop Host mode and Android admin/moderation console.
- XEP-0050 and provider-specific server administration where available.

Exit criteria: a Host instance serves public pages and share links; reports and
admin actions work end-to-end; Android provides the moderation console.

### Milestone 6 — Platform hardening, UX and release gate

Remaining:

- Android: manifest permissions, foreground service, FCM/UnifiedPush, XEP-0357
  delivery, notification channels, Keystore, share intents, deep links, PiP,
  audio routing, proximity, camera switching, signing and store metadata.
- Desktop: tray, autostart, single instance, keyring, file dialogs, PipeWire
  capture, AppImage/deb/rpm packaging and Host service packaging. Native
  notifications are implemented; add actions and verification.
- Web: wasm-safe transport, crypto/TLS and storage; WebSocket/BOSH discovery
  (XEP-0156) where needed.
- Navigation parity: Chats, News, Explore, Publish, Search, Notifications,
  Profile/Accounts, Admin.
- Responsive layout, themes, accent colours, font size, compact mode.
- i18n framework and community translations (do not copy Movim's AGPL
  catalogues).
- Accessibility: keyboard navigation, screen readers, focus management,
  contrast.
- Deep links, share targets, onboarding, help and account recovery.

Release gate:

- STATUS.md is green for the release platforms.
- Interop passes against Movim, Dino, Conversations, Galene/Galener and Host.
- Multi-account, multi-device, OMEMO, Spaces, Stories and calls have
  end-to-end tests.
- Offline/restart/process-death behaviour is verified.
- Secrets are not plaintext; TLS defaults are safe; logs are redacted.
- Documentation, ADRs and CI reflect reality.

## 4. Required XEP coverage

This is the target protocol set. Current per-XEP status is in STATUS.md §4.

### Core

RFC 6120/6121/6122/7590; XEP-0030, 0115, 0198, 0199, 0203, 0215, 0352, 0368,
0388, 0390.

### Identity, presence and contacts

XEP-0012, 0045 (full), 0048, 0050, 0054, 0077, 0080, 0084, 0100, 0107, 0108,
0118, 0153, 0157, 0172, 0191, 0292, 0319, 0392, 0402.

### Messaging and rooms

XEP-0004, 0059, 0085, 0184, 0201, 0203, 0231, 0249, 0280, 0297, 0308, 0313,
0333, 0334, 0359, 0363, 0372, 0380, 0393, 0410, 0424, 0425, 0428, 0433, 0444,
0461, 0469.

### PubSub, social, Spaces, Stories

XEP-0060, 0163, 0223, 0277, 0330, 0402, 0462, 0465, 0472, 0496, 0497, 0499,
0501, 0503.

### Encryption

XEP-0384 (1:1 and MUC), 0380, 0300.

### Calls

XEP-0166, 0167, 0176, 0272, 0320, 0338, 0339, 0353, 0215, 0507; Galener
interoperability and the ProtoXEP conference dialect.

### Push and platform

XEP-0357, 0198 resumption for mobile, 0352 CSI.

## 5. Testing and acceptance

- Unit tests for every parser and state machine.
- Golden XML fixtures for outgoing stanzas, including both conference dialects.
- Malformed-input and fuzz tests.
- SQLite migration tests from an empty database and from the Redb import path.
- Prosody integration: auth, roster, 1:1, carbons, MAM, MUC, bookmarks, PubSub,
  OMEMO, Spaces.
- Media integration: 1:1 audio/video, conference join, per-participant content,
  screen share, active speaker.
- Host integration: public routes, share metadata, report intake, moderation,
  admin authorization.
- CI matrix: Linux fmt/clippy/test, desktop build/smoke, Android build/emulator
  smoke, wasm build once supported. No step may claim a platform it does not
  exercise.

## 6. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Windows/Android media backend gaps | Calls unavailable on release platforms | Spike WebView WebRTC and native backends before building more UI |
| wasm crypto/TLS (`aws-lc-sys`) | Web target cannot build | Choose a wasm-safe rustls/crypto provider and a WebSocket transport |
| Plaintext secrets | Security and trust | Keyring/Keystore or documented SQLCipher threat model |
| CI does not match reality | False confidence | Install system deps; run service-backed suites; only claim exercised builds |
| Galener availability | Cannot validate SFU | Keep the local testbed; record golden stanzas |
| Experimental Spaces/Stories behaviour | Instability | Capability detection, graceful degradation, isolated modules |
| Scope breadth | Long delivery | Milestones with exit criteria; no UI before protocol correctness |
| AGPL contamination | Licensing | Clean-room; review translations/assets; document sources |

## 7. Decision gates

1. **Gate B — Media:** choose the Android backend (WebView WebRTC, native
   `webrtc-rs`, or hybrid) and record an ADR before Android call UI work.
2. **Gate C — Host model:** desktop-embedded service, separate headless binary,
   or both. Default: one binary with a `host` mode and a `client` mode.
3. **Gate D — Public content cache:** cache only public nodes or federate
   on-demand. Default: on-demand fetch plus TTL cache.
4. **Gate E — Admin scope:** confirm which XMPP server-level admin operations
   are required beyond XEP-0050 and provider commands.
5. **Gate F — Web transport:** WebSocket-only or WebSocket plus BOSH; which
   wasm storage and crypto stack.
6. **Gate G — Secret storage:** OS keyring/Keystore, SQLCipher, or both.

## 8. Immediate next actions

1. Validate the committed screen-share/portal fixes against the testbed.
2. Make CI install the system dependencies it needs and split honest jobs.
3. Verify the keyring path on GNOME and KDE, then add Android Keystore support.
4. Move secrets out of `settings`; add schema versioning and migrations.
5. Add notification actions/tray support and verify desktop toasts on GNOME,
   KDE and a headless session.
6. Validate Movim ↔ Rust through Galener and run the 10+ participant load test.
7. Pick the Android media backend and record the ADR (Gate B).
8. Implement MUC OMEMO and the remaining M2 XEP gaps.
9. Complete the feed/communities/Stories work, then the Host crate.
10. Start i18n and accessibility before the release gate.

## 9. Inputs needed from the project owner

- Test XMPP accounts on a server with MUC, MAM, upload, PubSub, OMEMO and push.
- A Movim/Galener deployment for interop and load runs (the in-repo testbed is
  the reference).
- A public domain and hosting decision for the Host/Gateway (Gate C).
- Android signing key and store/packaging requirements.
- Confirmation of the web transport and secret-storage decisions (Gates F/G).
