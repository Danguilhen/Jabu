# Implementation status

Last reconciled: 2026-11-08. See `git log` for the current commit.

This file is the **single source of truth** for what works today. `README.md`
is the entry point, `docs/PLAN.md` is forward-looking, and `AGENTS.md` is the
working guide for agents. When behaviour changes, update this file first and
make the others link to it instead of repeating it.

Status legend:

- **Works** — implemented and exercised (unit test, live test, or a documented
  manual test).
- **Partial** — the main path exists but a named part is missing.
- **Not started** — designed but not implemented.
- **Not supported** — deliberately out of scope for the current target.

## 1. Build and test evidence

Verified on Linux x86_64 inside `nix develop` with Rust 1.98.1:

| Check | Result |
|---|---|
| `cargo check -p xmpp-core -p storage --all-targets` | Passes |
| `cargo check -p media -p platform -p ui --all-targets` | Passes (one `unused_mut` warning in `ui`) |
| `cargo check --workspace --all-targets` | Passes with a fresh `CARGO_TARGET_DIR` |
| `cargo test -p xmpp-core -p storage --lib` | 277 + 33 tests pass |
| `cargo test -p jabu-i18n -p ui -p media -p platform --lib` | 8 + 27 + 9 + 6 tests pass |
| `cargo test -p host` | 54 tests pass |
| `cargo test -p xmpp-core --test golden --test malformed` | 4 tests pass, 27 golden fixtures |
| `cargo test -p media --test conference_client_flow --no-run` | Compiles |
| `cargo test -p xmpp-core --test prosody_smoke` | 25 live tests; gated on `JABU_TEST_XMPP=1` |
| `cargo check -p xmpp-core --target wasm32-unknown-unknown` | **Fails** in `aws-lc-sys` (`pthread_rwlock_t`) |
| `cargo test --workspace --all-targets` | Not completed; the run exhausted the local disk after the workspace check passed |
| Android build / emulator | Debug APK builds, launches and logs in against the local Prosody fixture; signed release APK installs and updates with a test key | Calls, push, Keystore and background delivery not exercised |
| GStreamer and SFU integration | 8 tests, 7 marked `#[ignore]`; the live ones need the testbed |

The hermetic suite is healthy. The expensive paths are all opt-in, and the
service-backed CI jobs are still being repaired (see §6).

The repository still contains vendored `prose-*`/`xso` crates under
`crates/vendor/`. They are patched into the workspace and are not first-party
code.

## 2. Platforms

| Platform | Status | Notes |
|---|---|---|
| Linux desktop | Works (alpha) | Main development target. Dioxus 0.7.10 WebView, GStreamer calls, Wayland/X11 screen capture. |
| Android | Partial | Debug APK builds, launches and logs in against the local Prosody fixture on an x86_64 API 34 emulator. Calls are compiled out (no media backend); no push, Keystore or foreground service yet. Signed arm64 releases publish from a `v*` tag (`v0.1.0` was the first); no CI emulator smoke test yet. |
| Web (WASM) | Not started | Target remains in scope. `xmpp-core` currently fails to compile for wasm and there is no WebSocket/BOSH transport or WASM storage adapter. |
| macOS | Untested | Expected to build; no CI job or manual validation. |
| Windows | Builds in CI | `dx bundle` produces an NSIS installer, and a portable zip bundles the GStreamer runtime so calls work without installing it. Compiles and packages on `windows-latest`; nothing has run on Windows hardware yet. |
| iOS | Not started | Target exists in the toolchain only. |

## 3. Feature status

### Accounts and connection

| Feature | Status | Notes |
|---|---|---|
| Login, TLS, STARTTLS, direct TLS | Works | TLS verification, CA file, client certificate/key, proxy settings |
| SASL mechanisms | Works | SCRAM-SHA-256/1, PLAIN, ANONYMOUS |
| Reconnect and backoff | Works | Configurable attempts; manual disconnect |
| Stream management | Works | XEP-0198 enable, ack, resume, retransmit |
| Multi-account sessions | Works (desktop, transport verified) | `AccountSession` per JID; the active account plus up to four saved accounts stay connected with separate buses, processors, persistence, notifications, call media and call recording. Switching swaps the UI client without reconnecting; background incoming calls notify and can be answered after switching. Removing an account signals its workers to stop. Two simultaneous sessions are verified against Prosody; the desktop UI was also run headlessly under Xvfb with an active and a background account, switching between them, delivering a message to the background session and switching to the callee during an incoming call to show the call dialog |
| Per-account preferences | Works (desktop) | Notification policy and the last presence show/status are stored per account and applied on switch; the Accounts list shows active/online/offline |
| In-band registration | Works | XEP-0077 |
| Ad-hoc commands / gateways | Partial | Discover and execute; multi-step command sessions not exposed |
| Credential storage | Works (desktop) | OS keyring via `keyring`; falls back to SQLite settings only when no keyring service is available, and never keeps both copies |
| OMEMO storage | Works (desktop) | AES-256-GCM at rest with a keyring-held per-account key; refuses to overwrite an encrypted state with plaintext when the keyring is unavailable |

### Contacts and identity

| Feature | Status | Notes |
|---|---|---|
| Roster, groups, subscriptions | Works | Approve/deny, pushes, revoke keeps the contact |
| Blocking | Works | XEP-0191 block/unblock, pushes, blocklist retrieval |
| vCard-temp and vCard4 | Works | Read-merge-write; profile editor writes both |
| Avatars | Works | XEP-0084 PEP avatar and XEP-0153 vCard avatar with presence hash |
| Presence menu | Works | Discord-style user panel with Online/Idle/DND/Invisible quick switch, custom status, priority and account switching; Invisible uses XEP-0186 and is disabled when the server does not advertise it |
| PEP activity | Works | Tune, mood, activity, location, vCard4 |
| Entity capabilities | Partial | Sends hashed `<c/>` with PEP notify features; stores contact caps; no caps cache or caps-driven disco |
| Last activity | Works | XEP-0012 |
| Consistent colours | Works | XEP-0392 |
| Shared media | Works (contact profile) | Image/video/file grid from the last 500 stored messages, deduplicated by URL; no room media gallery |

### 1:1 chat

| Feature | Status | Notes |
|---|---|---|
| Send/receive/persist/paginate | Works | MAM `max`/`before`, RSM cursor, archive-complete |
| Receipts, markers, chat states | Works | XEP-0184/0333/0085 |
| Replies, edits, retractions, reactions | Works | XEP-0461/0308/0424/0444 |
| Replies with quoted context | Works | Reply metadata is rendered |
| Mentions, forwarding, `/me`, attention | Works | XEP-0372/0297/0224 |
| Attachments | Works | XEP-0363 upload, OOB, file metadata, inline image/video/audio; desktop Save caches downloads |
| Emoji, custom emoji, stickers | Works | Noto SVG, packs, XEP-0231 BOB, reactions picker |
| GIF/Tenor | Not started | Deliberately deferred |
| Link previews | Works (desktop) | On-demand, off by default; public-host guard and in-memory cache |
| Drafts, pin, mute, archive, clear | Works | Stored in SQLite settings/JSON |
| Global search | Partial | Message FTS plus in-memory contacts, rooms, posts and spaces; no stories index |
| OMEMO | Works (1:1, MUC experimental) | XEP-0384 OMEMO 2 and legacy OMEMO 0.3 dual-stack, device lists, bundles, trust, persistence, grouped room keys; live interop passes |

### Rooms (MUC)

| Feature | Status | Notes |
|---|---|---|
| Join/leave/create, subjects | Works | |
| Bookmarks and autojoin | Works | XEP-0402 publish/read/retract, XEP-0469 pinning and legacy `storage:bookmarks` |
| Configuration and destroy | Works | Owner/admin forms |
| Affiliations, invites, private messages | Works | Kick/ban and role/affiliation changes |
| MAM, reactions, edits, retractions, replies | Works | |
| Rejoin after reconnect | Works | Rooms are marked disconnected and rejoined on the next stream |
| Self-ping | Works | XEP-0410 keepalive every 60 s; a failed ping marks the room stale |
| Message moderation | Works (desktop) | XEP-0425 retraction for room moderators; no reason prompt yet |
| Room discovery/search | Works (desktop) | XEP-0433 keyword search with an editable search service and RSM paging |
| MUC OMEMO | Partial (experimental) | Non-anonymous rooms; real-JID occupants and affiliations; per-device trust; OMEMO 2 and legacy group sending; counter-53 heartbeats and automatic key-exchange acknowledgements (group acks wait for every present member's key); member keys fetched on demand; missing-key warnings; live room interop passes |

### Calls and voice

| Feature | Status | Notes |
|---|---|---|
| 1:1 audio/video | Works (desktop) | Jingle, ICE, DTLS-SRTP, XEP-0353, camera switch, mute; completed/missed calls appear inline in the chat |
| 1:1 screen share | Works (desktop) | X11 and Wayland portal capture |
| Voice channels (SFU) | Works (desktop) | MUC room, SFU discovery, Galener and ProtoXEP dialects |
| Voice room preparation | Works (desktop) | Joins the room, appoints the SFU admin, invites it, then proposes |
| Participant tiles, mute, volume, active speaker | Works (desktop) | Per-content probes |
| Conference screen share | Works (desktop) | Second Jingle session; committed at `0125c6f5` |
| Group camera video | Not started | Voice channels are audio + screen share |
| Android calls/voice | Not started | No media backend |
| Rust SFU bridge | Not started | Galener (PHP) is the server-side reference for now |

### Spaces, feed, stories

| Feature | Status | Notes |
|---|---|---|
| Spaces discovery/create/join/leave | Works | XEP-0503 via pubsub service |
| Space metadata, avatar, banner | Works | |
| Space rooms, directories, members, affiliations | Works | Create/edit/delete, notify/pin, directory grouping and member approval; right-click menu for mark-read, pin, notification mode, timed/forever mute and leave |
| Space channel management | Works | Right-click on the channel column: hide muted channels, invite/share link, create category or channel (managers), link an existing room, space settings; per-channel mute is local to the account and persisted with the space |
| Space avatar and banner | Works | XEP-0503 image items published through HTTP Upload; managers can replace both |
| Space room updates | Works | Partial pubsub notifications are merged; refreshed on connect |
| Voice rooms inside spaces | Works (desktop) | Normal MUC rooms with a `voice` bookmark marker |
| Microblog feed | Partial | Cached account and contact posts with titles, tags, attachments, edit, delete/retract, RSM paging, comments, drafts, styled text and per-post notifications |
| Communities, explore, tags, comments, likes | Partial | Community node metadata/discovery, follow/unfollow, article and gallery pages, publishing, comments, likes, create/config/avatar/members, tags, contact suggestions and feed filters; public community pages remain |
| Stories | Works (desktop) | Publish from gallery or webcam with a 9:16 crop, 24 h expiry, chat/space rail, seen rings, auto-advancing viewer, delete/share/comment and notifications |
| Public blog and post pages | Works (Linux/server) | `jabu host` serves `/blog/{jid}` and `/post/{jid}/{id}` from public PubSub nodes with OpenGraph cards, ETag and a TTL cache |
| Public community pages | Works (Linux/server) | `/community/{service}/{node}` and `/community/{service}/{node}/{item}` with metadata, articles and Atom/RSS feeds |
| Public Space and Story pages | Works (Linux/server) | `/space/{service}/{node}` lists rooms and directories; `/story/{jid}/{id}` renders active Stories and 404s expired ones |
| Feeds, sitemap and robots | Works (Linux/server) | Atom 1.0 and RSS 2.0 for blogs and communities; `sitemap.xml` plus `robots.txt` |
| Contact pages and share links | Works (Linux/server) | `/contact/{jid}` with the public subscription list; canonical and `jabu://` deep links on every public page |
| Public rate limiting | Works (Linux/server) | Per-client fixed-window limit (`rate_limit_per_minute`, `trust_proxy`) returning 429 with `Retry-After` |
| Report intake and moderation | Works (Linux/server) | `/report` stores reports in SQLite; `/admin` hides, unhides, dismisses or reopens them and writes an audit entry; hidden targets disappear from every public page |
| Host administration | Works (Linux/server) | Hashed admin tokens with `admin`/`moderator` roles, 24 h HTTP-only sessions, JSON report/audit endpoints, same-origin CSRF checks, editable settings/branding and retention from `/admin/settings`, token management and `VACUUM INTO` backup downloads |
| Host deployment | Works (Linux/server) | Standalone `jabu-host` binary, systemd unit, Dockerfile/compose, Caddy and nginx examples, and `docs/HOST.md`; a Host release/packaging pipeline is not set up |

### Notifications, settings, UI

| Feature | Status | Notes |
|---|---|---|
| Notification events | Partial | 1:1 messages, space room messages (honouring the space notification mode and mute) and new microblog posts trigger desktop notifications; regular MUC rooms and mobile delivery are not implemented |
| Desktop notifications | Works (Linux) | Native `notify-rust` toasts; a missing notification daemon is logged as a warning |
| Chat scrolling | Works | Conversations open at the latest message, restore their previous position when switching back, and stay pinned to the bottom while new messages arrive |
| Tray, autostart, single instance | Works (desktop) | Tray with Show/Quit, XDG autostart and a runtime-socket single-instance guard; closing the window quits |
| Android push (FCM/UnifiedPush) | Not started | XEP-0357 registration state exists; no delivery |
| Themes | Works | Dark, light, silver, butter |
| Settings | Partial | Theme, language (system/English/French), timestamps, notifications, link previews, font size, compact spacing and a keyboard-shortcut reference; per-animation controls are not implemented |
| i18n | Works (desktop) | Fluent catalogues embedded in `crates/i18n`; the desktop UI and notification text are translated, with English and French shipped, system-locale detection and a persisted `ui.language`. Host pages, Android manifest strings and RTL layout remain |
| Accessibility, keyboard shortcuts | Partial | Focus-visible ring, skip link, landmarks and `aria-current`, named controls, modal focus trap/restore and Escape for every dialog, keyboard context menus, live regions for connection/errors/incoming calls/new messages, reduced motion, AA-tested theme tokens, and global shortcuts (Ctrl/Cmd+K, Ctrl/Cmd+,, Alt+1…5, F6). See `docs/ACCESSIBILITY.md`; Android screen readers and target-size auditing are untested |
| In-app reports and deep links | Works (desktop) | Report dialog for contacts, feed/community posts and Stories posts to the Host JSON API; `jabu --open jabu://…` focuses contacts, communities, Spaces, feeds and Stories |
| Public Host, reporting, moderation, admin | Partial | `crates/host` serves public blog, post, contact, community, Space and Story pages with feeds, sitemap, robots, deep links, rate limiting, reporting, a moderation queue with an audit log, role-based admin sessions, editable settings, retention and backup downloads. The app posts reports to `/api/report` and opens `jabu://` links; the Android console, federation/cache sharing and XEP-0050 administration are missing |
| Data export/import CLI | Not started | Commands print a message only |

## 4. XEP status

This is the current state, not the target. [ADR-018](adr/ADR-018-target-protocol-coverage.md)
defines the target set; `docs/PLAN.md` lists the open work.

### Core and connection

| XEP / RFC | Status | Notes |
|---|---|---|
| RFC 6120 / 6121 / 6122 / 7590 | Works | TCP, STARTTLS, direct TLS, SASL, roster, presence |
| XEP-0030 Service Discovery | Works | `disco#items`, `disco#info` |
| XEP-0115 Entity Capabilities | Partial | Sends hashed `<c/>` with microblog/avatar/activity/OMEMO `+notify`; no feature cache |
| XEP-0198 Stream Management | Works | Enable, ack, resume, retransmit |
| XEP-0199 Ping | Works | Both directions, pong timeout |
| XEP-0203 Delayed Delivery | Works | Parsed for MAM and live delays |
| XEP-0215 External Service Discovery | Works | STUN/TURN |
| XEP-0352 Client State Indication | Works | Active/inactive, restored after reconnect |
| XEP-0368 Direct TLS | Partial | Port 5223 works; no SRV `_xmpps-client._tcp` lookup |
| XEP-0390 Entity Capabilities 2.0 | Partial | Advertises a SHA-256 capability hash in a second `<c/>`, answers `disco#info`, queries and verifies remote hash nodes, and caches their features; server-side caps optimization is not implemented |
| XEP-0388 SASL2 | Not started | Design and fallback strategy in ADR-016; SASL1 remains active |

### Identity and contacts

| XEP | Status | Notes |
|---|---|---|
| XEP-0012 Last Activity | Works | |
| XEP-0050 Ad-Hoc Commands | Works (client) | List, execute/next/prev/complete/cancel, forms, notes and actions; contact-profile dialog. Server-wide/admin entry points are not exposed yet |
| XEP-0054 vCard-temp | Works | Read-merge-write |
| XEP-0077 In-Band Registration | Works | |
| XEP-0080 User Location | Works | PEP |
| XEP-0084 User Avatar | Works | Publish/fetch/SHA-1 cache |
| XEP-0100 Gateway Interaction | Works | Data-form and legacy registration |
| XEP-0107 / 0108 / 0118 | Works | Tune, activity, mood |
| XEP-0153 vCard-Based Avatars | Works | Presence advertises the SHA-1 photo hash; a changed hash fetches the vCard |
| XEP-0157 Server Contact Discovery | Works | |
| XEP-0172 User Nickname | Works | Carried in presence |
| XEP-0186 Invisible Command | Partial (server-gated) | IQ-based invisible/visible commands in the user panel; disabled when the server does not advertise `urn:xmpp:invisible:1`; session-only |
| XEP-0191 Blocking Command | Works | |
| XEP-0292 vCard4 | Works | Field-level merge |
| XEP-0319 Last User Interaction | Works (manual) | Incoming idle is parsed and shown; choosing Idle in the user panel sends an idle timestamp; no automatic OS idle detection |
| XEP-0392 Consistent Colour Generation | Works | |
| XEP-0402 PEP Native Bookmarks | Works | Autojoin |

### Messaging and rooms

| XEP | Status | Notes |
|---|---|---|
| XEP-0004 Data Forms | Works | Generic parse/submit |
| XEP-0059 RSM | Partial | `max`/`before` for MAM, `max`/`after` for the microblog feed |
| XEP-0085 Chat States | Works | |
| XEP-0184 Receipts | Works | |
| XEP-0201 Message Threads | Works (flat view) | Live and MAM parsing; replies keep or start a thread id, shown with a thread marker; no dedicated thread pane |
| XEP-0231 Bits of Binary | Works | Request/response cache, stickers |
| XEP-0249 Direct MUC Invitations | Works | |
| XEP-0280 Message Carbons | Works | Sent/received attribution |
| XEP-0297 Stanza Forwarding | Works | Forwarding between chats/rooms |
| XEP-0308 Last Message Correction | Works | 1:1 and MUC |
| XEP-0313 MAM | Works | 1:1 and MUC, `<fin>`, RSM cursor |
| XEP-0333 Chat Markers | Works | |
| XEP-0334 Message Hints | Works | `store`, `no-store`, `no-copy`, `markable` |
| XEP-0359 Unique Stanza IDs | Works | `origin-id`, `stanza-id`, dedupe |
| XEP-0363 HTTP Upload | Works (native) | WASM path untested |
| XEP-0372 References | Works | Mentions |
| XEP-0380 Explicit Message Encryption | Works | Outbound `<encryption>`; inbound namespaces shown for chats, rooms and MAM |
| XEP-0393 Message Styling | Works | Emphasis, strikethrough, monospace, links |
| XEP-0410 MUC Self-Ping | Works | 60 s keepalive; failed ping marks the room stale |
| XEP-0424 Message Retraction | Works | |
| XEP-0425 Message Moderation | Works (desktop) | Moderators retract others' room messages |
| XEP-0428 Fallback Indication | Works | Fallback bodies for reactions, edits, retractions and moderation are applied, not shown as messages |
| XEP-0433 Extended Channel Search | Works (desktop) | Keyword search and RSM paging against a search service |
| XEP-0444 Message Reactions | Works | |
| XEP-0461 Message Replies | Works | 1:1 and MUC |
| XEP-0469 Bookmark Pinning | Works | Pinned room bookmarks sort first in the room list; spaces keep their own pin model |

### PubSub, social, Spaces, Stories

| XEP | Status | Notes |
|---|---|---|
| XEP-0060 Publish-Subscribe | Partial | Item fetch/events; subscribe is used for microblog/spaces, not a generic client |
| XEP-0163 PEP | Works | Avatars, activity, vCard4, OMEMO devices |
| XEP-0223 Persistent Private Data | Partial | Used for bookmarks private storage |
| XEP-0277 Microblogging | Partial | Own posts with categories, enclosures, edit/delete and open comments nodes; reads and posts `rel=replies` comments |
| XEP-0330 Pubsub Subscription | Works (client) | Publishes and retracts spaces on the public PEP node and Movim's spaces mirror, and community follows on Movim's private mirror; identical Movim item ids and extensions; contact profiles list a user's public subscriptions |
| XEP-0402 PEP Native Bookmarks | Works | Publish, read, retract and autojoin |
| XEP-0462 PubSub Type Filtering | Works (client) | Include/exclude filter for `disco#items`; spaces use it when the service advertises the feature, then fall back to per-node `disco#info` |
| XEP-0465 Pubsub Public Subscriptions | Not started | Requires XEP-0376 PAM |
| XEP-0472 Social Feed | Partial | Microblog subset, XEP-0501 stories and XEP-0472 community nodes with discovery, publishing, comments and gallery feeds |
| XEP-0496 Pubsub Node Relationships | Works (client) | Parses `parent`/`link` metadata from every cached node and from metadata notifications, shows relationships in the space dialog and discovered-node list, and sets the parent on microblog comment nodes when the service supports it |
| XEP-0497 Pubsub Extended Subscriptions | Works (client) | Subscription type/depth options with a hierarchical subscribe action (this node, children, all descendants); linked-node items land on their own node and metadata notifications update relationships |
| XEP-0499 Pubsub Extended Discovery | Works (client) | Extended `disco#items` with relationship metadata and RSM paging; the space dialog discovers child/linked nodes and loads further pages |
| XEP-0501 Stories | Works (desktop) | Stories node with one enclosure, 24 h expiry, publish/retract, story references in 1:1 and MUC chat (plaintext or OMEMO), seen state, chat/space rail, viewer actions and desktop camera capture |
| XEP-0503 Server-side Spaces | Works (desktop) | Discovery/create/join/leave, rooms, directories, members and affiliations, avatar/banner, configuration; Movim's custom spaces subscription node is mirrored |

### Encryption and calls

| XEP | Status | Notes |
|---|---|---|
| XEP-0384 OMEMO | Works (1:1), experimental MUC | OMEMO 2 X3DH/ratchet/SCE plus legacy OMEMO 0.3 (`eu.siacs.conversations.axolotl`); live slixmpp-omemo tests pass for both stacks; the counter-53 heartbeat and the automatic key-exchange acknowledgement are sent for live traffic, the 1:1 acknowledgement falls back to the existing session when bundles are not cached, a message from an unknown device triggers a key fetch, and a MAM catch-up defers prekey deletion and answers after `<fin>` |
| XEP-0380 EME | Partial | See messaging |
| XEP-0300 Hash Functions | Partial | SHA-1/SHA-256 used where needed |
| XEP-0166 Jingle | Partial | 1:1 and voice conferences |
| XEP-0167 Jingle RTP | Partial | Opus audio, VP8 video, RTCP-mux |
| XEP-0176 Jingle ICE-UDP | Partial | Host/STUN/TURN, trickle ICE |
| XEP-0272 Muji | Not started | |
| XEP-0320 Jingle DTLS | Partial | DTLS-SRTP |
| XEP-0338 Jingle Grouping | Partial | BUNDLE |
| XEP-0339 Source-Specific Media | Partial | |
| XEP-0353 Jingle Message Initiation | Works | propose/accept/proceed/reject/retract/finish |
| XEP-0507 Content Categories | Partial | `speaker` / `slides` in conferences |

### Push and platform

| XEP | Status | Notes |
|---|---|---|
| XEP-0357 Push Notifications | Partial | Registration/unregistration only; no delivery |
| XEP-0198 resumption for mobile | Works | Not wired to an Android foreground service |
| XEP-0352 CSI | Works | Background hint only |

## 5. Storage

SQLite is the system of record (`crates/storage`), with Redb kept only for a
one-release settings import.

Current tables: `accounts`, `contacts`, `chats`, `messages`, `messages_fts`,
`muc_rooms`, `muc_messages`, `muc_messages_fts`, `pubsub_nodes`,
`pubsub_items`, `spaces`, `calls`, `settings`.

Known gaps:

- Forward-only migrations are applied when the database is opened. The schema
  is stamped with `PRAGMA user_version` and recorded in `schema_migrations`;
  the current version is 2. Future schema changes must add a migration.
- FTS exists for messages and MUC messages only. Posts, contacts, rooms,
  spaces and stories are not indexed.
- Drafts and chat preferences are JSON blobs in `settings`.
- Credentials use the OS keyring on desktop; when no keyring service is
  available they fall back to the SQLite `settings` table. A successful keyring
  write deletes any older plaintext copy, and a fallback value found while the
  keyring works is migrated into it and removed.
- OMEMO state is encrypted with AES-256-GCM when the keyring is available; the
  per-account key lives in the keyring. Without a keyring the state is
  plaintext only if it was never encrypted; an existing ciphertext is never
  replaced with plaintext. See §6 and ADR-015.

## 6. Known gaps and risks

### Security

1. **Secret storage depends on a keyring service.** Credentials fall back to
   SQLite when no keyring is available. OMEMO state is plaintext only if it was
   created without a keyring; once encrypted, a missing keyring makes the state
   unreadable and writes are refused rather than downgraded. Fallbacks are
   logged and documented, and a secret is never left in both stores. The GNOME
   Secret Service path is verified locally; KDE/KWallet and Android Keystore
   remain untested. If the Secret Service accepts reads but times out on both
   write and delete, an older keyring value can still shadow a newer fallback;
   the timeout is logged and the keyring entry must be cleared manually.
2. **Mobile notification payloads are not delivered.** The Android/iOS
   `NotificationService` implementation only logs; the desktop path uses
   `notify-rust`.
3. **Database encryption.** The database as a whole is not encrypted;
   SQLCipher remains an open decision. Only credentials and OMEMO state have
   keyring-backed protection.
4. **Log redaction** is not systematically tested. Normal-level logs avoid
   message bodies, but this needs an audit before release.
5. **Link previews make outbound HTTP requests.** They are off by default and
   only run when the user requests one. Loopback, private, link-local and
   `.local` targets are rejected, but DNS names that resolve to private
   addresses are not resolved and re-checked, and there is no per-site
   allowlist.
6. **OMEMO is interoperable with both OMEMO 2 and legacy OMEMO 0.3 peers but
   remains experimental.** OMEMO 2 uses the X3DH associated data
   initiator-first in both directions and the SCE payload matches
   `python-twomemo`; legacy OMEMO 0.3 is published and selected when a peer has
   no OMEMO 2 bundle. The OMEMO 2 SCE envelope carries random `rpad`
   padding plus `time`/`to`/`from` affixes, and `omemo-stanza`'s independent
   SCE codec parses it in a unit test. Legacy OMEMO 0.3 messages add a plain
   `<body>` fallback because peers such as Movim drop stanzas without one.
   Live slixmpp-omemo tests pass for 1:1 OMEMO 2, 1:1 legacy fallback and a
   non-anonymous MUC with OMEMO 2 and legacy group sending. Remaining limits:
   `slixmpp-omemo` 2.2.0 does not implement the XEP-0420 envelope, so the
   harness unwraps it at the session-manager level; MUC OMEMO is still
   restricted to non-anonymous rooms. Inbound
   `from`/`to` affixes are checked against the stanza routing; the optional
   `time` affix is not verified so MAM replays stay decryptable. Both OMEMO 2
   conformance items are implemented: the heartbeat for a first message on a
   ratchet key with a counter of 53 or higher (MUST) and an automatic empty
   acknowledgement after a key exchange (SHOULD). They are sent for live
   traffic and debounced per peer or room, so an inbound key exchange is
   answered without waiting for a user reply and a burst cannot become a storm.
   A 1:1 acknowledgement uses the existing session when the contact's bundles
   are not cached, a group acknowledgement waits until every present member has
   a key, a message from an unknown device triggers a device/bundle fetch, and
   a device removed from our own PEP list re-announces itself. An archived
   key exchange keeps its one-time prekey until the MAM catch-up finishes and
   is answered with an empty message right after the `<fin>`, as §6 requires;
   the bundle is republished once the prekey is consumed. End-to-end exchange
   with Movim's own browser client was not achieved: Movim fails
   inside its `ChatOmemoStorage.isTrustedIdentity`/`SessionBuilder` path before
   it builds a session (see `dev/reference-testbed/README.md`).

### CI and release engineering

5. The workflow runs on every push to `main`/`develop` and on `v*` tags.
   `check`, `test`, the Prosody smoke job, `build-desktop`, `build-windows`
   and `build-android` pass on GitHub Actions. Tag pushes were previously
   ignored because only `branches` was declared.
6. The `build-web` job is informational and no longer gates releases; the wasm
   target still fails in `aws-lc-sys`.
7. The Android debug APK builds, launches and logs in against the local Prosody
   fixture on an x86_64 API 34 emulator, but CI still only builds: there is no
   emulator or device smoke test and no Android media backend. The signed
   release job runs on a `v*` tag, reads the four `ANDROID_*` repository
   secrets and publishes an arm64 APK; `v0.1.0` is the first release with a
   signed `jabu-0.1.0.apk`.
8. CI does not run the Prosody or GStreamer integration suites; they require
   services and are opt-in. The Prosody fixture job installs the distribution
   package and starts its own instance.

### Architecture

9. The `AppService`/`AccountManager` boundary from ADR-008 is not implemented.
   The UI still owns connection lifecycle and calls `XmppClient` directly.
   Treat this as a target refactor, not as the current design.
10. Every saved account (active plus up to four background sessions) is connected at launch with its own event bus, processor, persistence and notification workers; switching swaps the active `XmppClient` without reconnecting. Call media and call recording now run per session; an incoming call on a background account notifies instead of opening the dialog until the user switches. The desktop UI has been exercised under Xvfb with two live accounts (both connected, switched, a message delivered to the background session, and switching to the callee during an incoming call) using debug-only `JABU_AUTO_SWITCH`, `JABU_AUTO_SEND`, `JABU_AUTO_CALL` and `JABU_AUTO_QUIT_AFTER` hooks. Android multi-account testing is blocked: the AVD exists but no usable emulator binary is installed.
11. `crates/host` serves public blog, post, contact, community, Space and Story
    pages with Atom/RSS feeds, sitemap, robots, deep links, rate limiting,
    report intake, a moderation queue, role-based admin sessions, editable
    settings/branding, retention and backup downloads, and ships as the
    standalone `jabu-host` binary. The app submits reports and opens `jabu://`
    links; the Android console is missing.
12. Group camera video is not implemented; voice channels are audio plus screen
    share.
13. The accessibility baseline is in place for the desktop/web UI (see
    `docs/ACCESSIBILITY.md`); Android screen-reader behaviour, target-size
    auditing and the remaining hover-only controls are open.
14. Internationalization covers the desktop UI and notifications in English
    and French (see `docs/I18N.md`). Host pages, Android manifest strings,
    community translations and RTL layout are open.

### Product scope still open

15. Community nodes, discovery, following, pages, publishing, management and
    contact suggestions are implemented; public community pages and host-side
    share links remain.
16. GIF/Tenor, attachment thumbnails and download progress are not implemented.
17. MUC OMEMO is experimental: it requires non-anonymous rooms and only
    encrypts for members whose bundles are already cached. It supports OMEMO 2
    and legacy group sending, key-only heartbeats and a missing-key warning so
    the user can opt out and send plaintext. Broader client coverage is still
    needed.
18. Desktop packaging and notification actions are missing.
19. Android push, foreground service and Keystore are missing.

## 7. Test inventory

| Suite | Count | How to run |
|---|---|---|
| `xmpp-core` unit tests | 277 | `cargo test -p xmpp-core --lib` |
| `storage` unit tests | 33 | `cargo test -p storage --lib` |
| `media` unit tests | 9 | `cargo test -p media --lib` |
| `ui` unit tests | 18 | `cargo test -p ui --lib` |
| `host` unit tests | 54 | `cargo test -p host` |
| `platform` unit tests | 6 | `cargo test -p platform --lib` |
| Golden stanzas | 27 fixtures, 2 tests | `cargo test -p xmpp-core --test golden` |
| Malformed/fuzz | 2 tests | `cargo test -p xmpp-core --test malformed` |
| Real OS keyring | 1 ignored test | `JABU_REAL_KEYRING=1 cargo test -p storage --lib real_os_keyring_round_trip -- --ignored` |
| Multi-account live | 1 test | `JABU_TEST_XMPP=1 cargo test -p xmpp-core --test multi_account_smoke` |
| Prosody live | 25 tests | `JABU_TEST_XMPP=1 cargo test -p xmpp-core --test prosody_smoke -- --test-threads=1` |
| OMEMO interop | 4 ignored tests, passing | `JABU_TEST_XMPP=1 JABU_TEST_OMEMO_PYTHON=… cargo test -p xmpp-core --test omemo_interop -- --ignored` (OMEMO 2 1:1, legacy 1:1 fallback, MUC OMEMO 2 and MUC legacy; see `dev/omemo-interop/README.md`) |
| GStreamer/SFU live | 8 tests, 7 ignored | `JABU_TEST_XMPP=1 cargo test -p media --test conference_live -- --ignored --nocapture` |
| UI / app / platform | 18 UI tests; 6 platform tests | UI tests cover emoji, packs, shared-media dedupe, deep-link parsing, report-target selection and account-switch resolution; platform tests cover report request serialization |

The Prosody suite covers login, direct TLS, proxy, IQ routing, MAM, blocking,
room create/configure/destroy, room join, reconnect/rejoin, upload, registration,
vCard merge, last activity, spaces refresh and space-room notifications.

## 8. How to update this file

- Change the status only after a test or a documented manual run.
- Add the commit hash and date at the top.
- Keep `README.md`, `docs/PLAN.md` and `AGENTS.md` free of duplicated status;
  they should link here. The target XEP set lives in ADR-018, not here.
- If a feature moves from Partial to Works, add the test that proves it.
