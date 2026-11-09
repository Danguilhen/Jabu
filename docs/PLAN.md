# Jabu — remaining work

Status: living backlog, reconciled 2026-11-08.

Scope and release order: [ADR-017](adr/ADR-017-parity-scope-and-release-order.md).
Target protocol set: [ADR-018](adr/ADR-018-target-protocol-coverage.md).
Decisions and crate map: [adr/README.md](adr/README.md),
[README.md](../README.md#architecture). State and test evidence:
[STATUS.md](STATUS.md). Accessibility gaps: [ACCESSIBILITY.md](ACCESSIBILITY.md).

**How to update this file:** open work only. Delete a bullet when it is done;
do not append progress notes. Features, per-XEP status and test evidence go to
`STATUS.md`. A decision changing reality is edited in the owning ADR in place.

## 1. Release gate

- STATUS.md is green for the release platforms.
- Interop passes against Movim, Dino, Conversations, Galene/Galener and Host.
- Multi-account, multi-device, OMEMO, Spaces, Stories and calls have
  end-to-end tests.
- Offline/restart/process-death behaviour is verified.
- Secrets are not plaintext; TLS defaults are safe; logs are redacted.
- Documentation, ADRs and CI reflect reality.

## 2. Open milestones

Each milestone ends with a demo against a real service and an updated
`STATUS.md`.

### Milestone 0 — Foundation and release engineering

Remaining:

- Make every CI job honest: install GTK/WebKit/GStreamer in check/test jobs,
  add wasm once `xmpp-core` builds for it, keep Android build-only until an
  emulator smoke exists, and run the service-backed suites in jobs that start
  their services.
- Verify the keyring path on KDE/KWallet and add Android Keystore support.
- Decide the wasm transport (WebSocket via `tokio-xmpp` or a separate wasm
  transport) and the crypto/TLS replacement for `aws-lc-sys`.
- Keep `cargo fmt`, `cargo clippy` and hermetic tests green on the pinned
  toolchain.

Exit criteria: a clean checkout builds, tests and packages without undeclared
system dependencies; secrets are not plaintext; migrates an old database.

### Milestone 1 — Accounts, identity, presence and contacts

Remaining:

- Finish per-account settings for the remaining global options.
- Android login, permission and notification flow.
- Contact suggestions and richer group management where Movim parity needs it.

Exit criteria: several identities, accounts, presence and contacts can be
managed on desktop and Android without reconnecting other accounts.

### Milestone 2 — Complete chat and rooms

Remaining:

- MUC OMEMO: broaden client coverage and interoperability beyond the current
  testbed.
- Full RSM for the remaining PubSub collections (spaces and PEP lists).
- GIF/Tenor integration (blocked on a provider decision).
- Attachment thumbnails and download progress.
- Android message pipeline and notification integration.

Exit criteria: 1:1 and MUC parity on desktop, plus a usable Android chat path
with encryption and media.

### Milestone 3 — Social: feed, communities, Spaces, Stories

Remaining:

- Complete XEP-0277: rich XHTML content and the remaining feed parity.
- Extend XEP-0462 type filtering to the remaining node listings.
- XEP-0465 public subscriptions (needs XEP-0376 PAM first).

Exit criteria: feed, communities, Spaces and Stories work against a
Movim-compatible service, including public pages and share links.

### Milestone 4 — Calls and voice

Remaining:

- Validate the screen-share/portal fixes against the testbed on X11 and
  Wayland.
- Group camera video in SFU conferences.
- Android media backend (WebView WebRTC or native), permissions, audio focus,
  foreground service and lifecycle handling.
- Movim ↔ Rust interop through Galener, and Libervia ↔ Rust through ProtoXEP.
- 10+ participant load, network switch, screen-off and process-death tests.
- Replace the manual `calls.sfu` override with automatic discovery; keep the
  override only as a fallback.
- Rust SFU bridge: XEP-0114 component, MUC presence/COIN, Galène admin API and
  WebSocket translation, emitting both dialects.
- Remove the interim `GaleneVoicePort` path once the bridge or a supported SFU
  makes it unnecessary.

Exit criteria: desktop ↔ Android 1:1 calls; 10+ participant voice channels with
screen share and group video; verified Movim and ProtoXEP interop.

### Milestone 5 — Public host, moderation, reporting, administration

Remaining:

- Report actions for chat messages and 1:1/MUC users through a server-side
  moderation service, where one exists.
- Desktop Host mode and the Android admin/moderation console.
- A Host release/packaging pipeline and a public-domain/hosting plan.
- Cross-instance federation or shared public caches.
- XEP-0050 and provider-specific server administration: add a server settings
  entry point and admin command shortcuts.

Exit criteria: reports and admin actions work end-to-end; Android provides the
moderation console.

### Milestone 6 — Platform hardening, UX and release gate

Remaining:

- Android: manifest permissions, foreground service, FCM/UnifiedPush,
  XEP-0357 delivery, notification channels, Keystore, share intents, deep
  links, PiP, audio routing, proximity, camera switching, signing and store
  metadata.
- Desktop: keyring, file dialogs, PipeWire capture, AppImage/deb/rpm and Host
  service packaging; notification actions and verification.
- Web: wasm-safe transport, crypto/TLS and storage; WebSocket/BOSH discovery
  (XEP-0156) where needed.
- Navigation parity: Chats, News, Explore, Publish, Search, Notifications,
  Profile/Accounts, Admin.
- Responsive layout, themes, accent colours.
- Native review of machine-assisted translations, optional Traditional
  Chinese and European Portuguese catalogues, and Android
  manifest/notification-channel strings (do not copy Movim's AGPL
  catalogues).
- Accessibility remainder: Android screen-reader verification, 24×24 target
  sizes, remaining hover-only actions, automated accessibility-tree check.
- Deep links, share targets, onboarding, help and account recovery.

### Milestone 7 — On-device translation

Implemented: the `crates/translate` core, pinned model manifest, checksummed
download/import, GGUF preflight, SQLite translation cache, desktop manual and
global/per-chat automatic translation, idle model unload, Vulkan/Metal GPU
offload, an optional helper-process backend, model settings/download, and an
optional loopback endpoint.
See [TRANSLATION.md](TRANSLATION.md) and
[ADR-020](adr/ADR-020-on-device-translation.md).

Remaining:

- Ship the embedded llama.cpp backend on Android with memory, thermal and
  lifecycle policy; validate the 1.8B Q4 model on low-, mid- and high-end
  devices.
- Add global translation search and a "translate conversation" batch action
  with progress/cancel.
- Harden the runtime: finish macOS/Windows sandboxing, verify sidecar packaging
  in the AppImage/installers, add CPU rlimits, and decide whether to vendor the
  Candle backend.
- Verify the external endpoint against Ollama and `llama-server`, including
  token handling and the remote consent flow.
- Native review of the translation UI strings and the model output quality for
  the supported language pairs.
- Add a CI job that builds `--features embedded` and a gated model smoke test;
  keep the default build free of C++.

## 3. Testing and acceptance

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

## 4. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Windows/Android media backend gaps | Calls unavailable on release platforms | Record the Android backend in ADR-010 before building more UI |
| wasm crypto/TLS (`aws-lc-sys`) | Web target cannot build | Choose a wasm-safe provider and a WebSocket transport |
| KDE/KWallet and Keystore unverified | Secrets unavailable or unsafe on those platforms | Verify KWallet; implement Keystore; keep the documented fallback |
| CI does not match reality | False confidence | Install system deps; run service-backed suites; only claim exercised builds |
| Galener availability | Cannot validate SFU | Keep the local testbed; record golden stanzas |
| Experimental Spaces/Stories behaviour | Instability | Capability detection, graceful degradation, isolated modules |
| Scope breadth | Long delivery | Milestones with exit criteria; no UI before protocol correctness |
| AGPL contamination | Licensing | Clean-room; review translations/assets; document sources |

## 5. Open decision gates

1. **Gate B — Media:** choose the Android backend (WebView WebRTC, native
   `webrtc-rs`, or hybrid) and edit ADR-010 before Android call UI work.
2. **Gate E — Admin scope:** confirm which XMPP server-level admin operations
   are required beyond XEP-0050 and provider commands.
3. **Gate F — Web transport:** WebSocket-only or WebSocket plus BOSH; which
   wasm storage and crypto stack.

Gates C (host and client modes in one binary plus the standalone `jabu-host`) and
D (on-demand fetch plus a TTL cache) are resolved; see
[ADR-009](adr/ADR-009-public-host.md). Gate G is resolved for desktop (keyring
with a documented fallback, [ADR-015](adr/ADR-015-omemo-at-rest.md)); Android
Keystore remains open.

## 6. Immediate next actions

1. Choose the Android media backend and edit ADR-010 (Gate B).
2. Make CI install its system dependencies and run service-backed suites.
3. Verify the keyring path on KDE/KWallet, then add Android Keystore support.
4. Implement group camera video in SFU conferences.
5. Validate screen sharing on X11 and Wayland.
6. Broaden MUC OMEMO client coverage.
7. Close the social gaps: XEP-0277 rich XHTML, XEP-0462/0465.
8. Add the remaining high-confidence catalogues and native review.
9. Do the Android accessibility pass (screen reader, target sizes).

## 7. Inputs needed from the project owner

- Test XMPP accounts on a server with MUC, MAM, upload, PubSub, OMEMO and push.
- A Movim/Galener deployment for interop and load runs (the in-repo testbed is
  the reference).
- A public domain and hosting plan for the Host deployment.
- Android signing key and store/packaging requirements.
- Confirmation of the web transport and crypto/storage choices (Gate F).
