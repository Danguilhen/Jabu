# Jabu — Full Feature Parity Plan

Status: Draft
Target: Full user-facing Movim parity, release-blocking on Linux desktop and Android
Reference implementation: Movim (AGPLv3) — behavior/spec reference only, clean-room implementation

## 1. Decisions locked by this plan

1. **Full parity is the goal**, including public-facing pages, instance administration, moderation, user reporting, public sharing links, Spaces, Stories and group video.
2. **Linux desktop and Android are the priority platforms.** Web and iOS may follow after the desktop/Android release gate.
3. **Voice channels use the XMPP SFU model, never the Galène WebSocket API.** The shipped client stores no Galène URL, group or password. It joins the MUC, discovers the SFU and calls it with Jingle. Galener's current dialect is implemented first for Movim interoperability; the ProtoXEP *Jingle Audio/Video Conferences* dialect follows for Libervia and future clients. A Rust Galener-compatible SFU bridge is a follow-up; Galener (PHP) is used server-side for the testbed and initial deployments.
4. **Storage is not tied to redb.** The plan switches application data to SQLite + FTS5, with OS-backed secret storage for credentials and keys.
5. **Dioxus upgrade is attempted early.** Prefer the current stable Dioxus 0.7 line if desktop, Android, router and WebView integration are compatible. Never mix 0.6 and 0.7 crates. If a blocking incompatibility is found, freeze on 0.6.3 and record the reason in an ADR.
6. **Public pages and administration require a host surface.** A pure mobile/desktop app cannot serve public URLs. This plan adds an optional, headless **Host/Gateway** mode built from the same Rust crates. The desktop app can run it; Android connects to it as a client and provides the mobile admin/moderation console. Existing Galener/Galène servers remain external dependencies.

## 1.1 Progress log

- Dioxus upgraded from 0.6.3 to 0.7.10; desktop build passes.
- `xmpp-core` no longer depends on Dioxus; UI state moved to `ui::ClientState`; startup order fixed.
- SQLite backend with FTS5 added; `StorageFactory` now creates SQLite by default.
- Initial persistence wiring: roster, chats, messages, MUC rooms/messages and PubSub items load/save.
- Live message events now emitted; desktop notification listener can receive them.
- Origin/stanza IDs added; carbon attribution, MUC echo duplication, MUC self-role/affiliation and subject persistence fixed.
- Roster subscription/block actions are forwarded to the connection; incoming subscription requests surface as events.
- Incoming ping and roster pushes are answered; OMEMO decrypt prefers encrypted payloads and includes the local device.
- MUC bookmarks (XEP-0402 and legacy `storage:bookmarks`) are fetched on login and autojoined.
- First-run configuration now works without a `config.toml`; partial configs use defaults.
- ADR-006 through ADR-010 added.
- Removed the stray `dioxus-router` 0.6 dependencies from `app` and `ui`; the workspace now resolves a single Dioxus 0.7.10 line (M0.1).
- Prosody 13 fixture is runnable: `prosody/run.sh` starts a local server with alice/bob/admin, MUC, MAM, carbons, bookmarks (XEP-0402), PEP and HTTP file upload; `prosody/stop.sh` stops it (M0.7).
- Connection now negotiates STARTTLS explicitly, honors the CA file, client certificate/key and verification flags, installs a single rustls `CryptoProvider`, detects missing XEP-0199 pongs and reconnects after the stream ends; XEP-0198 is only requested when configured (M0.4 partial).
- `crates/xmpp-core/tests/prosody_smoke.rs` logs in to the fixture and rejects a bad password; the CI `xmpp-smoke` job runs it against Prosody 13 (M0.7 partial).
- XEP-0198 resumption is implemented: the client enables SM with `resume`, keeps an unacked stanza queue, requests acknowledgements, resumes the previous stream after a reconnect and retransmits what the server did not acknowledge (M0.4 partial).
- The smoke suite now also verifies that Prosody offers a resumable stream-management session (M0.7 partial).
- Transports honour proxy settings (SOCKS5 with optional username/password, HTTP/HTTPS CONNECT with Basic auth) and `direct_tls` connects straight to port 5223 with TLS (XEP-0368 fallback port; SRV `_xmpps-client._tcp` still needs a DNS resolver) (M0.4 partial).
- XEP-0215 external services are queried after disco advertises `urn:xmpp:extdisco:2` and STUN/TURN endpoints are kept in `DiscoManager`; XEP-0352 CSI state is sent when the server supports `urn:xmpp:csi:0` and restored after reconnect (M0.4 partial).
- The smoke suite covers direct TLS, a SOCKS5 proxy and an IQ round-trip against Prosody (M0.7 partial).
- Typed IQ routing (`IqRouter`) matches replies to request ids, enforces per-request timeouts, maps `<iq type="error">` to a typed error and fails every request in flight on disconnect (M0.4 partial).
- Messages keep both XEP-0359 `origin-id` and `stanza-id`; live, carbon and MAM copies deduplicate across all identities, and carbon `sent`/`received` direction is attributed correctly (M0.5 partial).
- MUC echoes are suppressed by id or by a short nick/body window when the server returns no id; own messages never raise unread counts (M0.5 partial).
- XEP-0313 paging tracks the query id and IQ id, records the `<fin>` archive-complete flag and the RSM `first` cursor, and exposes it through `MamManager`; outgoing messages carry `store` and `markable`, while receipts and markers are `no-store`/`no-copy` (M0.5 partial).
- Chat/MUC updates now emit real events for sent messages, receipts, edits, retractions and reactions, and sent messages are persisted (M0.5 partial).
- The Prosody smoke suite archives a message, queries MAM and waits for the completed page (M0.5/M0.7 partial).
- Roster subscriptions track `ask`/pending direction, approve/deny update the correct half of `to`/`from`, revoked subscriptions keep the contact, and roster pushes are acknowledged while `subscription="remove"` removes only that entry (M0.6 partial).
- XEP-0191 blocking sends well-formed `block`/`unblock` IQs, applies server pushes, retrieves the authoritative blocklist on connect/disco and persists `is_blocked` through the existing contact store (M0.6 partial).
- The smoke suite exercises a block/unblock round-trip and blocklist retrieval against Prosody's `mod_blocklist` (M0.6/M0.7 partial).
- The Prosody fixture now also exposes a generic PubSub component, unlocked instant rooms, MUC archive and the HTTP upload service; server `disco#info` is requested alongside `disco#items` so features and upload discovery actually arrive (M0.7 partial).
- 22 golden stanzas under `crates/xmpp-core/tests/golden/` parse and drive processor state (messages, carbons, MAM, receipts, markers, reactions, edits, retractions, MUC, roster, blocking, disco, extdisco, stream management, bookmarks) (M0.7 partial).
- `tests/malformed.rs` feeds malformed stanzas and 5000 deterministic fuzz cases through the parser, asserting no panics (M0.7 partial).
- Live integration now covers two-client 1:1 with live/MAM deduplication, MUC create/join/group message, HTTP upload, MAM completion, blocklist, proxy, direct TLS, IQ routing and stream-management resumption (M0.7 partial).
- CI no longer sets `-D warnings` globally or for clippy while the workspace warning backlog is cleaned up; jobs only claim checks and builds they actually run. Android emulator smoke remains deferred (M0.7 partial).
- In-band registration (XEP-0077) opens a fresh unauthenticated stream, fetches the form and registers; the smoke suite registers a unique account and logs in with it (M1 partial).
- vCards are stored whole as vCard-temp XML and updated read-merge-write, replacing only fields present in the patch; the Prosody test proves a second update keeps the earlier `ORG`. vCard4 (XEP-0292) is parsed into the same field names, merged by field and preferred over vCard-temp when both exist; the profile editor edits individual fields and writes both payloads (M1 partial).
- XEP-0012 last activity is queried and stored as `last_seen`; presence updates now carry `show`/`status`/`priority` and the XEP-0172 nick from the vCard, with a shared presence builder (M1 partial).
- XEP-0392 identity colours are deterministic per JID/nickname and covered by unit tests (M1 partial).
- PEP user activity is implemented for tune (0107), mood (0118), activity (0108) and location (0080), plus vCard4 (0292) publish/fetch; incoming PEP items are stored on the contact and a live test publishes from alice and fetches from bob (M1 partial).
- Entity capabilities (XEP-0115): presence carries a hashed `<c/>` node, incoming caps are stored per contact, and XEP-0157 server contact addresses are parsed into `DiscoManager` (M1 partial).
- Ad-hoc commands (XEP-0050) are discoverable and executable through the typed IQ router, with a live Prosody admin-command test. Gateway interaction (XEP-0100) discovers components, parses both `jabber:x:data` and legacy `jabber:iq:register` forms, and registers or removes an account from the Gateways dialog (M1 partial).
- Roster groups and display names can be edited through `set_contact_groups`/`set_contact_name`; the contacts UI has a profile drawer with presence resources, vCard/vCard4 fields, PEP activity, caps node, group editing and block/chat/refresh actions (M1 partial).
- Accounts are stored with per-account credentials; the Accounts dialog can add, remove and switch accounts, reloading contacts, chats, rooms, OMEMO state and the active account setting. The MVP keeps one connection active at a time and scopes 1:1 and MUC history per account (M1 partial).
- Presence can be changed from the user menu (online/away/DND/extended away, custom status and priority), and the contacts tab has a blocked-contacts view alongside the existing block/unblock action (M1 partial).
- M2 chat organization started: drafts are saved per account and conversation (1:1 and MUC), pinned chats sort first, 1:1 conversations can be muted or archived from the chat list, and the list has unread and archived filters. Chat preferences survive restarts, and muted chats no longer raise desktop notifications (M2 partial).
- Room owners/admins can fetch and submit the XEP-0045 owner configuration form and destroy a room from the room configuration dialog; a generic XEP-0004 form parser/submitter backs it. XEP-0224 attention requests can be sent from a 1:1 chat, are stored as an attention message and highlighted when received; rooms have an invite dialog that sends mediated invitations with an optional reason. File upload accepts multiple files, uploads them sequentially with per-file progress, and can stop the remaining queue (the in-flight request still completes). Room mentions (XEP-0372) are generated for `@nick` tokens, parsed from incoming live/MAM messages, and highlighted when they refer to the local user; `/me` actions are formatted with the sender's nick or localpart (M2 partial).
- Messages can be forwarded (XEP-0297) from a 1:1 chat to another chat or room; incoming forwarded messages keep the original sender and timestamp, and the UI labels them. Uploaded files now send XEP-0363 URLs with `jabber:x:oob` and `urn:xmpp:file:metadata:0` metadata; incoming OOB/file metadata is parsed for live, MAM and room messages, and the UI renders images, video, audio and file links with sizes. Upload progress/cancel and multi-file selection are still open (M2 partial).
- Emoji support started: the composer has a searchable Noto Emoji SVG picker with recent/category state, message text renders emoji as SVG images, and 1:1/MUC reactions now support a full picker, counts, own-reaction highlighting, toggling/removal and reactor names. Custom emoji packs can be imported from Movim `meta.json` v2 archives and are rendered from `:alias:` tokens; XEP-0231 BOB request/response caching, sticker pack import and sticker sending/rendering are implemented, with shortcode autocomplete, an alias/pack management UI, per-account favorite emoji, skin-tone variants and server-advertised reaction restrictions. Emoji, stickers and reactions are complete except GIFs/Tenor, which are out of scope for this milestone.
- Global search covers local 1:1 and room messages through the account-scoped FTS indexes, plus contacts and rooms, from a sidebar dialog. Clearing a conversation removes its local messages and MAM cursor while leaving the server archive untouched; searches and clears are account-scoped (M2 partial).
- Still open in M1: simultaneous sessions for several accounts at once.
- Spaces services are discovered from pubsub components and space creation works on `pubsub.<domain>`; the local Prosody fixture allows node creation for registered users.
- An interim direct-Galène voice-channel mode proved the SFU media path (10+ participants, active speaker, per-participant volume), but it is not the target architecture and will be removed in M4.4–M4.9.
- The desktop stylesheet is embedded and re-injected into the document body so the signed-in shell keeps its styling when Dioxus reconciles the head.
- The SFU target is Galener-compatible MUC voice rooms with no client-side Galène configuration.
- The Galener dialect is captured as golden fixtures (COIN presence, disco, proceed, session-accept, transport-info, content-add/remove, content-accept, session-info) and `xmpp-core::conference` now parses both Galener `<jingle-participant>` and ProtoXEP `<origin>` stanzas, discovers the SFU from room presence, and builds propose/content-accept/mute/terminate stanzas (M4.4/M4.5 partial). The processor routes conference Jingle IQs and JMI messages by session id/SFU JID, room presence updates the discovery state, and `XmppClient::start_voice_conference` sends the propose. On `proceed` the processor creates the GStreamer offer and sends `session-initiate`; it installs the SFU answer, forwards candidates, and answers each `content-add` with a media-generated `content-accept`. The GStreamer backend merges dynamic receive contents by mid. Participant tiles, active-speaker/volume UI and the removal of the direct-Galène mode are still open.

## 2. Definition of parity

Parity is measured against Movim's current widget tree and `doap.xml`, adapted where the original feature is inherently server-side.

Parity is not "the UI looks similar". A feature is complete only when:

- It works against a real XMPP server, with a second client (Movim web, Dino or Conversations).
- It survives app restart and Android process death.
- It is account-scoped and multi-account safe.
- It degrades gracefully when the server lacks the required XEP.
- It has automated tests or a documented manual interop test.

The public/admin adaptation is:

| Movim server feature | Jabu adaptation |
|---|---|
| Public blog, community, space, post, story, contact pages | Optional Host/Gateway serves server-rendered public pages from XMPP/PubSub data, with OpenGraph, Atom/RSS and share URLs |
| Public sharing links | Host URLs (`https://host/...`) plus app universal/deep links (`jabu://...`) |
| Instance administration | Host admin console: users/roles, sessions, instance settings, storage, retention, federation/cache policy, audit log |
| Content moderation | Host moderation queue plus room/space/node-level moderation in the app |
| User reporting | Report action in apps, report intake and queue in Host, actions and audit trail |
| XMPP server administration | XEP-0050 ad-hoc commands and provider-specific commands where available; not a replacement for the XMPP server's own admin API |

## 3. Target architecture

```
crates/
  xmpp-core/    Protocol engine only. No Dioxus dependency.
                connection, typed IQ router, roster, chat, MUC, PubSub/PEP,
                OMEMO, Jingle control plane, spaces, stories, calls signalling.

  storage/      SQLite + FTS5 system of record.
                Migrations, account scoping, message/post/space/story indexes,
                search, moderation/report tables, call history, drafts.

  media/        NEW. CallMedia abstraction.
                WebviewMedia (Android WebView WebRTC; desktop if WebKitGTK supports it)
                NativeMedia fallback (webrtc-rs + platform audio/video) if required.

  host/         NEW. Optional headless Host/Gateway.
                Axum HTTP server, public pages, Atom/RSS, share links, report intake,
                moderation queue, admin console, instance settings, audit log.

  platform/     Desktop and Android backends only behind cfg.
                notifications, keyring/Keystore, files, permissions,
                foreground service, push, intents, screen capture.

  ui/           Dioxus presentation only. Subscribes to app service events.

  app/          Composition root, runtime, account manager, routing, Host mode switch.
```

### 3.1 Core rules

- `xmpp-core` must compile and run headless, with no Dioxus types.
- UI never calls protocol methods directly. It sends commands to an `AppService` and renders events.
- Every XMPP client instance belongs to exactly one account; storage and event routing are account-scoped.
- A typed IQ router correlates requests with results/errors and applies timeouts. No fire-and-forget IQ where a response matters.
- Every persistent write is account-scoped and transactional.
- Experimental XEPs are isolated behind capability detection and feature flags.

### 3.2 Account and session model

- `AccountManager` owns one `AccountSession` per configured JID.
- Each session has its own XMPP connection, OMEMO state, storage namespace, notification settings and event stream.
- The target keeps multiple accounts online at once, matching Movim; the current MVP switches the active account sequentially on one connection.
- Account switching changes the visible workspace without reconnecting the other accounts.

## 4. Data model

SQLite is the system of record. Schema is versioned with forward-only migrations.

Core tables:

- `accounts`, `account_settings`, `credentials_ref`
- `roster_items`, `roster_groups`, `presence_cache`, `blocked_jids`
- `conversations`, `messages`, `message_reactions`, `message_receipts`, `message_edits`
- `drafts`
- `muc_rooms`, `muc_bookmarks`, `muc_messages`, `muc_occupants_cache`
- `pubsub_nodes`, `pubsub_items`, `pubsub_subscriptions`
- `feed_posts`, `feed_comments`, `feed_reactions`, `feed_subscriptions`
- `spaces`, `space_items`, `space_memberships`
- `stories`, `story_views`, `story_reactions`
- `calls`, `call_participants`, `call_events`
- `omemo_identity`, `omemo_sessions`, `omemo_trust`, `omemo_devices`
- `notifications`, `reports`, `moderation_actions`, `admin_users`, `admin_sessions`, `audit_log`
- `push_registrations`
- `settings`

Search:

- SQLite FTS5 virtual tables for messages, posts, comments, rooms, spaces, stories and contacts.
- One FTS index per account database, rebuilt from source tables on migration.
- Search results carry enough context to open the exact conversation/post/room.

Sensitive data:

- Credentials and OMEMO private keys are not stored as plaintext in SQLite.
- Desktop: OS keyring (`keyring` crate or Secret Service/KWallet).
- Android: Android Keystore through JNI.
- Optional SQLCipher for the database if it does not compromise Android build reliability. Otherwise, encrypt only secret material and document the threat model.

Migration from the current Redb store:

- Import credentials, UI settings and OMEMO state.
- Import nothing else, because the current app does not persist contacts/messages/rooms.
- Keep a read-only Redb import path for one release, then remove it.

## 5. Milestones

Timelines are indicative and depend on the Dioxus and media spikes. Each milestone ends with a demo against a real server and an updated parity checklist.

### Milestone 0 — Foundation, blockers and architecture (first priority)

Exit criteria: app launches on desktop and Android, persists data, reconnects, passes integration tests; the Dioxus and media decisions are recorded.

#### M0.1 Dioxus upgrade spike

- Evaluate current stable Dioxus 0.7 with:
  - desktop (WebView) build and JS interop;
  - Android build through `dx`, Gradle, SDK/NDK;
  - router, signals, assets, hot reload;
  - WebView JS evaluation/IPC needed for WebRTC and native bridges.
- Check all workspace dependencies for version conflicts.
- Migrate if no blocking incompatibility; otherwise freeze on 0.6.3 and write `ADR-007`.
- Remove `dioxus` features for platforms not live (web) to prevent accidental mixed builds.

#### M0.2 Core/UI separation

- Remove Dioxus from `xmpp-core`.
- Replace `Signal` fields in `Roster`, `Chat`, `MucRoom`, `PubSubNode` with `Arc<RwLock<T>>` and change streams.
- Introduce `AppService`, `AccountManager`, command/event types.
- Fix startup/runtime ownership:
  - no `Signal::new` before `launch`;
  - no `tokio::spawn` before a runtime exists;
  - connection lifecycle owned by the app runtime.

#### M0.3 SQLite storage and migrations

- Replace Redb as the system of record.
- Use `rusqlite` with bundled SQLite and FTS5 enabled; validate Android cross-compilation.
- WAL mode, transactions, schema versioning, backup/export.
- Account-scoped databases or account-scoped tables; choose one and document it.
- Wire every read/write path in roster, chat, MUC, PubSub, OMEMO, feed, spaces, stories, calls, notifications, reports and admin.
- Add FTS5 indexes and search APIs.

#### M0.4 Network correctness

- Honor resource, priority, TLS verification/CA/client certificate and proxy settings.
- XEP-0368 direct TLS; XEP-0215 external services (STUN/TURN).
- Correct reconnect state machine, backoff, attempt limits and manual disconnect.
- XEP-0198 resumption with an unacked stanza queue.
- XEP-0352 Client State Indication for Android background behavior.
- XEP-0199 ping handling in both directions, with pong timeout detection.
- Typed IQ router with request ids, timeouts, result/error mapping and roster-push acknowledgement.

#### M0.5 Message pipeline

- XEP-0359 `origin-id` and `stanza-id`.
- Correct carbon `sent`/`received` attribution.
- MUC echo suppression and own-message unread handling.
- Live/MAM deduplication; RSM cursors; MAM `<fin>` and archive-complete state.
- XEP-0334 hints: `store`, `no-store`, `no-copy`, `markable`.
- Correct receipts and chat markers.
- Emit real events for messages, MUC messages, receipts, reactions, edits, retractions and calls.

#### M0.6 Roster and subscription correctness

- Send roster subscribe/approve/deny/unsubscribe through the connection.
- Answer roster pushes with IQ results.
- Surface incoming subscription requests; do not auto-approve silently.
- Blocking via XEP-0191 with `blocklist` retrieval and persistence.
- Keep roster item when subscription is revoked; only change subscription state.

#### M0.7 Test infrastructure

- Prosody fixture: two accounts, MUC component, HTTP upload, MAM, carbons, OMEMO, bookmarks, pubsub.
- Galene + Galener-compatible fixture for later call work.
- Golden XML fixtures for every implemented XEP.
- Parser fuzz tests and malformed-stanza tests.
- Integration tests for connect, roster, 1:1, MUC, MAM, upload, carbons, OMEMO.
- Android emulator smoke test.
- CI must only claim builds it actually performs.

### Milestone 1 — Accounts, identity, presence and contacts

Exit criteria: a user can manage identities, accounts, presence and contacts on desktop and Android.

- Multi-account add/remove/switch, per-account settings.
- Login, registration (XEP-0077), password change, account deletion where supported.
- XEP-0050 ad-hoc commands; XEP-0100 gateways; XEP-0157 server information.
- vCard-temp and XEP-0292 vCard4: read-merge-write, never destructive.
- XEP-0084 avatars, XEP-0153 vCard avatars, banners.
- XEP-0172 nick, XEP-0115 entity capabilities.
- Presence menu: online, away, DND, XA, custom status, priority.
- XEP-0012 last activity.
- XEP-0080 location, XEP-0107 tune, XEP-0108 activity, XEP-0118 mood.
- Roster groups, subscription requests, contact suggestions, blocked list UI.
- XEP-0392 consistent color generation for avatars/identities.
- Contact profile drawer: presence, resources, vCard, keys, shared media, actions.

### Milestone 2 — Complete chat and rooms

Exit criteria: 1:1 and MUC chat match Movim's feature set, including encryption, media, search and persistence.

#### 1:1 chat

- Send, receive, persist, paginate, search.
- Replies with quoted context, edits, retractions, reactions with picker and removal.
- Receipts, read markers, chat states, mentions, forwarding, `/me`, attention.
- Message hints and correct MAM behavior.
- Structured attachments: XEP-0363 v1, file/OOB metadata, inline images/video/audio, thumbnails, download cache.
- Upload progress, cancel, multiple files, size limits, filename handling.
- Emoji picker, custom emoji packs, favorites/aliases, shortcode autocomplete and Noto Emoji SVG rendering.
- Sticker packs sent through XEP-0231 Bits of Binary, with HTML `cid:` payloads and fallback bodies.
- GIFs/Tenor are deferred until the Host/GIF-provider work.
- Link previews with privacy controls and cache.
- Mute, pin, archive, clear history, drafts, unread filters.
- Global search across messages, contacts, rooms, posts, spaces, stories and tags.

#### Emoji, stickers and reactions

- Render Unicode emoji with bundled Noto Emoji SVG assets, shortcode titles and glyph fallback.
- Emoji picker with favorites, recent, categories, search, skin-tone variants and a common quick set.
- Composer insertion and `:shortcode:` / `:alias:` autocomplete.
- Import Movim-compatible custom emoji packs (`meta.json` v2), cache images locally and resolve `:alias:` tokens.
- Per-account favorite emoji and aliases with an Emoji settings page.
- Reaction picker with favorites, suggested emoji, common set and search.
- Reaction counts, own-reaction highlighting, add/remove toggling, MUC reactor names and server-advertised restrictions.
- Sticker packs with local import, XEP-0231 BOB advertise/respond/fetch/cache, and Movim-compatible `cid:` sticker messages.
- Preserve custom emoji, reactions and stickers through MAM and restart.
- GIFs/Tenor are explicitly out of scope for this milestone.

#### MUC and rooms

- XEP-0048 and XEP-0402 bookmarks: save, autojoin, favorites, room list persistence.
- Room discovery and search (XEP-0433), room metadata, room avatars.
- Room creation wizard and configuration forms.
- Affiliations, invitations, invitation management, destroy room, private messages.
- Self-presence, role/affiliation, status codes 110/201/301/307/321/322.
- Moderation: kick, ban, role/affiliation changes, XEP-0425 message moderation.
- XEP-0410 self-ping.
- MUC reactions, edits, retractions, replies, receipts, chat states, MAM, search.
- MUC OMEMO: sender devices, trust, multi-device, key rotation.
- XEP-0272 Muji room advertisement for calls.

#### OMEMO hardening

- Encrypt to the peer's devices **and** the user's other devices.
- Merge device lists instead of overwriting them.
- Prefer encrypted payloads over fallback plaintext bodies.
- Trust policy for undecided/new devices; fingerprint verification and QR.
- Persist identity/session/trust robustly; handle device list changes.
- OMEMO for MUC.
- Test against Movim, Conversations and Dino.

### Milestone 3 — Social: feed, communities, Spaces and Stories

Exit criteria: feed, communities, Spaces and Stories work against Movim-compatible services on both platforms.

#### Feed and communities

- Aggregate own and subscribed contact/community feeds.
- Subscribe/unsubscribe to microblog nodes; ingest PEP events.
- XEP-0277 publishing with Markdown, rich content, attachments, tags, titles.
- Comments and likes via comments nodes/reactions.
- Edit/delete/retract posts; drafts with autosave.
- Per-post notifications, mentions and likes.
- Explore: communities, tags, server discovery, suggestions.
- Public/private blog configuration; Atom/RSS; public share links through Host.
- XEP-0330/0465 subscription advertisement where supported.

#### Spaces (XEP-0503)

- Discover spaces services via `disco#items`, with XEP-0462 type filtering when available.
- Fetch space metadata via `disco#info`; content via pubsub items.
- Room items via XEP-0402 bookmarks; other entity types via OOB or XEP-0330.
- Join/leave as pubsub subscription; XEP-0497/0499 when available.
- Space avatar/banner metadata items.
- Space page: rooms, links, members, description, configuration.
- Create/configure/manage spaces where the service allows it.
- Advertise parent space for MUC rooms where applicable.

#### Space voice channels

- Voice rooms are normal MUC rooms (XEP-0045) with a `voice` marker in the XEP-0402 bookmark.
- Joining a voice room joins the MUC; text history remains available exactly like a Movim room.
- Owners can invite the server's SFU component into the room.
- The client discovers the SFU and starts the conference; no Galène URL, base group or password is configured in the client.
- Detailed protocol work is in M4.4–M4.9.

#### Stories (XEP-0501)

- Publish image/video stories with 24h expiry.
- Story ring, viewed state, own-story management.
- Private reactions/replies via direct messages.
- Viewer list and notifications.
- Story pages and share links through Host.

### Milestone 4 — Calls: 1:1, mesh and SFU conferences

Exit criteria: desktop↔Android 1:1 calls; 10+ participant voice channels through a Galener-compatible SFU; screen share, mute, active speaker and per-participant volume; interop with Movim verified; ProtoXEP conference interop tested.

#### M4.0 SFU testbed (do first)

- In-repo `dev/reference-testbed/` with setup/run scripts and a README.
- Nix provides PHP 8.4, Composer, PostgreSQL and Galène.
- Prosody advertises an external component `sfu.localhost` on port 5347.
- Movim is cloned and configured with `GALENER_XMPP_HOST`, `GALENER_XMPP_PASSWORD` and `GALENER_GALENE_PATH`.
- Start Movim, the Galener worker and Galène; log in a Movim web client.
- Capture golden stanzas: SFU presence with COIN `conference-info`, disco/caps, `session-initiate`/`session-accept`, `content-add`/`content-remove`, `transport-info`, mute/unmute.
- These fixtures drive the client implementation; no protocol guessing.

#### M4.1 Media backend spike (before UI work)

Implement a `CallMedia` trait and decide the backend:

1. **WebView WebRTC** via JavaScript bridge:
   - Android Chromium WebView: primary candidate.
   - Desktop WebKitGTK: verify camera, microphone, screen capture and `RTCPeerConnection` on supported distributions.
2. **NativeMedia fallback** using `webrtc-rs` with GStreamer/PipeWire capture and a rendering surface if WebKitGTK is insufficient.
3. If both are needed, keep the Jingle/SDP control plane identical and swap only the media backend.

No call UI is built until this spike has a working audio path on desktop and Android.

#### M4.2 Jingle control plane

- XEP-0166 Jingle.
- XEP-0167 RTP audio/video.
- XEP-0176 ICE.
- XEP-0338 group framing.
- XEP-0339 source.
- XEP-0320 DTLS.
- XEP-0353 Jingle Message Initiation for 1:1 calls: propose/ringing/proceed/reject/retract/finish.
- XEP-0298 COIN conference information (Galener).
- XEP-0507 content categories.
- XEP-0215 STUN/TURN discovery and refresh.
- SDP ↔ Jingle conversion, with fixtures from Movim/Dino/Conversations/Galener.

#### M4.3 One-to-one calls

- Audio/video call, mute, camera toggle, camera switch.
- Speaker/earpiece/Bluetooth routing.
- Call timer, call history, missed-call notifications.
- Incoming call UI from chat, room, notification and lock screen.
- Reject with reason; terminate on network loss; clean up stale sessions.
- Android foreground service and audio focus.

#### M4.4 Voice rooms, SFU discovery and invitation

- Create/join voice rooms as MUC rooms; keep the `voice` bookmark marker.
- Discover the SFU:
  - Galener: COIN `conference-info` in MUC presence and `urn:xmpp:coin:1` caps; call the SFU virtual user JID.
  - ProtoXEP: `urn:xmpp:jingle:av-conferences:0` and the `conference/audio-video` identity; call the room/service JID.
- Owner flow to invite the SFU component; show “SFU not enabled on this server” otherwise.
- Remove `calls.galene_url`, `calls.galene_group`, `calls.galene_password` and the direct Galène mode from the client and Settings.

#### M4.5 Galener conference dialect (Movim interoperability)

- `session-initiate` with audio (and later video/screen), `senders="initiator"`, BUNDLE.
- `session-accept`, `content-add`, `content-remove`, `transport-info`, `session-terminate`.
- Parse `<jingle-participant xmlns="{https://movim.eu}jingle_participant" participant="…">` and XEP-0507 `<category name="speaker|slides">`.
- Handle Galener's second Jingle session for screen share.
- COIN participant list for names and JIDs.
- `content-accept`/`content-reject`; mute/unmute via `session-info`.
- One `webrtcbin` per session with dynamic receive contents.

#### M4.6 ProtoXEP conference dialect

- Parse `<origin xmlns="urn:xmpp:jingle:av-conferences:0" entity="xmpp:…">`.
- One Jingle session per participant; `content-add`/`content-remove`.
- Ad-hoc configuration node `urn:xmpp:av-conferences:config`.
- Interop with Libervia and any standard SFU.

#### M4.7 Conference media and UI

- Dynamic receive contents on the existing `webrtcbin`; no one-pipeline-per-peer.
- Per-content level probe for active speaker and per-participant volume.
- Participant tiles from COIN/origins: audio, camera, screen share (`slides`).
- Mute/unmute, leave, participant count and connection state.
- Android needs the same signaling with its native media backend.

#### M4.8 Interoperability and load tests

- Our client ↔ Movim through Galener.
- Our client ↔ Libervia through the ProtoXEP SFU where available.
- 10+ participants, mute, per-participant volume, active speaker, screen share.
- Golden-fixture tests for both dialects in `xmpp-core` and `media`.

#### M4.9 Follow-up: Rust SFU bridge (no PHP)

- Rust XEP-0114 component with MUC presence and COIN.
- Galène admin API for groups/users and WebSocket signaling translation.
- Emit **both** the Galener dialect and the ProtoXEP `<origin>` so Movim and standard clients interoperate.
- Reuse `crates/media/src/galene.rs` as the Galène protocol library.
- Replaces Galener server-side; the client and UI remain unchanged.


### Milestone 5 — Public host, moderation, reporting and administration

Exit criteria: a Host instance serves public pages and share links; reports and admin actions work end-to-end; Android provides the admin/moderation console.

#### M5.1 Host/Gateway crate

- New `crates/host` binary, reusing `xmpp-core`, `storage` and shared domain types.
- Axum HTTP server with TLS termination guidance or reverse-proxy configuration.
- Public routes:
  - `/` landing;
  - `/blog/{jid}`;
  - `/post/{server}/{node}/{id}`;
  - `/community/{server}/{node}`;
  - `/space/{service}/{node}`;
  - `/story/{jid}/{id}`;
  - `/contact/{jid}`;
  - `/about`, `/report`, `/admin`.
- Server-rendered HTML with OpenGraph/Twitter cards, canonical URLs, Atom/RSS, sitemap and robots.
- Public content cache populated from XMPP/PubSub; no dependency on a Movim PHP instance.
- Access control: public, unlisted, private; honor node affiliations where the server exposes them.
- Share links and universal/deep links for Android and desktop.
- Rate limiting, caching, ETag/Last-Modified and content retention.

#### M5.2 Reporting and moderation

- In-app "Report" actions for users, messages, posts, rooms, spaces and stories.
- Host report intake with reporter identity, target, reason and evidence snapshot.
- Moderation queue with status, assignee, notes and actions.
- Actions: hide from public cache, remove cached content, notify reporter, escalate to node/MUC admins, block domain/JID from public cache.
- Audit log for every moderation action.
- Privacy: reporter anonymity, retention windows, abuse prevention.

#### M5.3 Instance administration

- Host admin authentication and roles.
- Instance settings: branding, public domains, registration policy, federation/cache policy, storage limits, retention, push keys, allowed upload services.
- User/session management for Host users; role assignment.
- Content moderation dashboard, reported content, audit log.
- Storage and backup management.
- Metrics and health endpoints.
- Server-level XMPP administration through XEP-0050 ad-hoc commands where the server supports it; document limitations for servers without an admin API.

#### M5.4 Admin in the apps

- Desktop: full Host admin console when running Host mode or connecting to a Host.
- Android: moderation queue, reports, user/role management, instance settings where practical; heavy configuration can remain desktop-first but must be reachable on Android.
- Role-aware UI: ordinary users never see admin surfaces.

### Milestone 6 — Platform hardening, UX and release gate

#### Android

- Manifest permissions: INTERNET, CAMERA, RECORD_AUDIO, MODIFY_AUDIO_SETTINGS, POST_NOTIFICATIONS, FOREGROUND_SERVICE_CAMERA/MICROPHONE, WAKE_LOCK, battery optimization exemption.
- Foreground service for connection and calls; survive screen-off/doze.
- FCM or UnifiedPush registration; XEP-0357 enable/disable; notification channels; per-chat/mute/mention behavior.
- Android Keystore for credentials and OMEMO keys.
- Share intents, file picker, camera, microphone, deep links, back navigation, rotation, process death.
- PiP, audio routing, proximity sensor, camera switching for calls.
- Signing, Play Store manifest, privacy policy, crash handling.
- Emulator and physical-device test matrix.

#### Desktop

- Native notifications, tray, autostart, single instance, keyring, file dialogs.
- Screen capture through PipeWire/portal.
- AppImage/deb/rpm packaging.
- Host mode packaging and service management.

#### UX and product

- Navigation: Chats, News, Explore, Publish, Search, Notifications, Profile/Accounts, Admin (role-gated).
- Responsive layout for desktop and phone.
- Themes: dark, light, system; accent colors; font size; compact mode.
- Full i18n framework with community translations. Do not copy AGPL Movim translation catalogs.
- Accessibility: keyboard navigation, screen readers, focus management, contrast.
- Deep links and share targets.
- Onboarding, help, about, account recovery.
- Keyboard shortcuts on desktop.

#### Release gate

The desktop/Android release is blocked until:

- The parity checklist is green for both platforms.
- Interop passes against Movim, Dino, Conversations, Galene/Galener and the Host.
- Multi-account, multi-device, OMEMO, Spaces, Stories and calls have end-to-end tests.
- Offline/restart/process-death behavior is verified.
- Security review is complete.
- Documentation, ADRs and CI reflect reality.

## 6. Required XEP coverage

This is the target protocol set. Current partial implementations are listed so they are not mistaken for complete.

### Core and connection

- RFC 6120, RFC 6121, RFC 6122, RFC 7590
- XEP-0030 Service Discovery
- XEP-0115 Entity Capabilities
- XEP-0198 Stream Management (including resumption)
- XEP-0199 Ping
- XEP-0203 Delayed Delivery
- XEP-0215 External Service Discovery
- XEP-0352 Client State Indication
- XEP-0368 Direct TLS
- XEP-0388 SASL2, XEP-0390 where supported

### Identity, presence and contacts

- XEP-0012 Last Activity
- XEP-0045 MUC (full)
- XEP-0048 Bookmarks
- XEP-0050 Ad-Hoc Commands
- XEP-0054 vCard-temp
- XEP-0077 In-Band Registration
- XEP-0080 User Location
- XEP-0084 User Avatar
- XEP-0100 Gateway Interaction
- XEP-0107 User Mood
- XEP-0108 User Activity
- XEP-0118 User Tune
- XEP-0153 vCard-Based Avatars
- XEP-0157 Server Contact Discovery
- XEP-0172 User Nickname
- XEP-0191 Blocking Command
- XEP-0292 vCard4
- XEP-0319 Last User Interaction
- XEP-0392 Consistent Color Generation
- XEP-0402 PEP Native Bookmarks

### Messaging and rooms

- XEP-0004 Data Forms
- XEP-0059 Result Set Management (full)
- XEP-0184 Message Delivery Receipts
- XEP-0201 Best Practices for Message Threads
- XEP-0203 Delayed Delivery
- XEP-0231 Bits of Binary
- XEP-0249 Direct MUC Invitations
- XEP-0280 Message Carbons
- XEP-0297 Stanza Forwarding
- XEP-0308 Last Message Correction
- XEP-0313 Message Archive Management
- XEP-0333 Chat Markers
- XEP-0334 Message Processing Hints
- XEP-0359 Unique and Stable Stanza IDs
- XEP-0363 HTTP File Upload
- XEP-0372 References
- XEP-0380 Explicit Message Encryption
- XEP-0393 Message Styling
- XEP-0410 MUC Self-Ping
- XEP-0424 Message Retraction
- XEP-0425 Message Moderation
- XEP-0428 Fallback Indication
- XEP-0433 Extended Channel Search
- XEP-0444 Message Reactions
- XEP-0461 Message Replies
- XEP-0469 Bookmark Pinning

### PubSub, social, Spaces, Stories

- XEP-0060 Publish-Subscribe
- XEP-0163 PEP
- XEP-0223 Persistent Storage of Private Data
- XEP-0277 Microblogging
- XEP-0330 Pubsub Subscription
- XEP-0402 PEP Native Bookmarks
- XEP-0462 PubSub Type Filtering
- XEP-0465 Pubsub Public Subscriptions
- XEP-0472 Social Feed
- XEP-0496 Pubsub Node Relationships
- XEP-0497 Pubsub Extended Subscriptions
- XEP-0499 Pubsub Extended Discovery
- XEP-0501 Stories
- XEP-0503 Server-side Spaces

### Encryption

- XEP-0384 OMEMO (1:1 and MUC)
- XEP-0380 Explicit Message Encryption
- XEP-0300 Use of Cryptographic Hash Functions
- Correct prekey handling, device trust, fingerprint verification and session persistence

### Calls

- XEP-0166 Jingle
- XEP-0167 Jingle RTP Sessions
- XEP-0176 Jingle ICE-UDP Transport Method
- XEP-0272 Multiparty Jingle (Muji)
- XEP-0320 Jingle DTLS
- XEP-0338 Jingle Grouping Framework
- XEP-0339 Source-Specific Media Attributes
- XEP-0353 Jingle Message Initiation
- XEP-0215 External Service Discovery (STUN/TURN)
- Galener Jingle SFU interoperability; optional Galene-direct adapter if deployment reality requires it

### Push and platform

- XEP-0357 Push Notifications
- XEP-0198 resumption for mobile reconnect
- XEP-0352 CSI for background transitions

## 7. Testing and acceptance strategy

### Automated

- Unit tests for every parser and state machine.
- Golden XML fixtures for outgoing stanzas.
- Malformed-input and fuzz tests.
- SQLite migration tests from an empty database and from the Redb import path.
- Integration tests against Prosody: auth, roster, 1:1, carbons, MAM, MUC, bookmarks, PubSub, OMEMO.
- Host integration tests: public routes, share metadata, report intake, moderation actions, admin authorization.
- CI matrix: Linux fmt/clippy/test, desktop build/smoke, Android build/emulator smoke.
- No CI step may claim a platform build that is not actually exercised.

### Interop

- Movim web instance (current stable).
- Dino and Conversations for 1:1, MUC, OMEMO, reactions, replies, edits.
- Galene + Galener deployment for SFU calls.
- A second Jabu instance for multi-device and multi-account tests.

### Manual device matrix

- Linux: GNOME/Wayland and X11, PipeWire screen share, notifications, tray.
- Android: current and previous major versions, phone and tablet, emulator and one physical device.
- Calls: desktop↔Android, Android↔Android, 1:1, 3+ SFU, screen share, mute, DTMF, network switch Wi-Fi↔mobile, screen off, process death.

## 8. Security and privacy

- TLS verification by default; explicit, documented override for self-signed test servers.
- Credentials and OMEMO keys in OS keyring/Keystore.
- Database encrypted or secret material separately protected; document the threat model.
- Log redaction: never log message bodies, passwords, tokens or key material at normal levels.
- Android permission prompts with clear rationale.
- Public Host pages honor content visibility and unlisting.
- Report evidence and reporter identity retention policy.
- Admin actions audited.
- Dependency and license audit before release.
- Clean-room process: XEPs and observed behavior only; no copied AGPL code, templates, translations or assets.

## 9. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Dioxus 0.7 Android/desktop incompatibility | Blocks migration | Timeboxed spike in M0; fallback to 0.6.3 with ADR; never mix versions |
| Desktop WebKitGTK lacks usable WebRTC | Blocks desktop calls | Spike early; NativeMedia fallback with `webrtc-rs` |
| Android background restrictions | Missing messages/calls | Foreground service, XEP-0198 resume, CSI, FCM/UnifiedPush |
| Jingle interoperability differences | Calls fail with Movim | XML/SDP fixtures and interop tests against Movim/Dino |
| Galener deployment unavailable for testing | Cannot validate SFU | Stand up local Galene/Galener fixture; request test accounts from user |
| Public Host infrastructure complexity | Blocks public pages/admin | Keep Host optional and separable; desktop can host; Android is a client |
| Experimental Spaces/Stories behavior | Instability | Capability detection, graceful degradation, isolated modules |
| SQLite/FTS5 Android build issues | Blocks persistence/search | Validate bundled SQLite/FTS5 early; fallback to Android system SQLite |
| Scope breadth | Long delivery | Milestones with hard exit criteria; no UI before data/protocol correctness |
| AGPL contamination | Licensing | Clean-room; review translations/assets; document sources |

## 10. Decision gates

1. **Gate A — Dioxus:** after the M0.1 spike, choose 0.7 or freeze 0.6.3. Record in ADR.
2. **Gate B — Media:** after the M4.1 spike, choose WebView WebRTC, NativeMedia, or hybrid. Record in ADR.
3. **Gate C — Host model:** confirm Host runs as a desktop-embedded service, a separate headless binary, or both. Default: one binary with a `host` mode and a `client` mode.
4. **Gate D — Public content cache:** decide whether Host caches only public nodes or also federates publicly on demand. Default: on-demand fetch plus TTL cache.
5. **Gate E — Admin scope:** confirm which XMPP server-level admin operations are required beyond XEP-0050 and provider-specific commands.

## 11. Immediate next actions

1. Build `dev/reference-testbed/`: Nix PHP 8.4, PostgreSQL, Galène, Movim and a Prosody `sfu.localhost` external component (M4.0).
2. Capture real Galener presence/COIN, disco and Jingle stanzas as golden fixtures.
3. Implement MUC-backed voice rooms, SFU discovery and the owner invitation flow (M4.4).
4. Implement the Galener conference dialect and media mapping (M4.5), then the ProtoXEP dialect (M4.6).
5. Port active speaker, per-participant volume, mute and screen share to conference contents (M4.7).
6. Remove the direct Galène client mode, its settings and the interim `VoicePort` UI path.
7. Verify Movim ↔ our client through Galener and run 10+ participant load tests (M4.8).
8. Plan the Rust SFU bridge for a PHP-free deployment (M4.9).
9. Continue the remaining milestone checklists and convert them into tracked issues.

## 12. Inputs needed from the project owner

- One or more test XMPP accounts, ideally on a server with MUC, MAM, upload, PubSub, OMEMO and push.
- A local Galene/Galener test deployment is approved; Galener may be run server-side in PHP for the testbed and initial deployments.
- Movim interoperability is required, so the Galener conference dialect is the reference; the ProtoXEP dialect follows.
- The direct Galène client mode and its connection settings are removed from the shipped client.
- Public domain and hosting decision for the Host/Gateway.
- Android signing key and store/packaging requirements.
- Confirmation of the Host deployment model at Gate C.
