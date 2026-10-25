# Implementation status

Last reconciled: 2026-11-06, at commit `0125c6f5`.

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
| `cargo test -p xmpp-core -p storage --lib` | 141 + 19 tests pass |
| `cargo test -p ui -p media -p platform --lib` | 7 + 9 + 0 tests pass |
| `cargo test -p xmpp-core --test golden --test malformed` | 4 tests pass, 27 golden fixtures |
| `cargo test -p media --test conference_client_flow --no-run` | Compiles |
| `cargo test -p xmpp-core --test prosody_smoke` | 7 live tests; gated on `JABU_TEST_XMPP=1` |
| `cargo check -p xmpp-core --target wasm32-unknown-unknown` | **Fails** in `aws-lc-sys` (`pthread_rwlock_t`) |
| `cargo test --workspace --all-targets` | Not completed; the run exhausted the local disk after the workspace check passed |
| Android build / emulator | Not verified |
| GStreamer and SFU integration | 8 tests, 7 marked `#[ignore]`; the live ones need the testbed |

The hermetic suite is healthy. The expensive paths are all opt-in, and the
workspace's CI jobs are not currently a reliable signal (see §6).

The repository still contains vendored `prose-*`/`xso` crates under
`crates/vendor/`. They are patched into the workspace and are not first-party
code.

## 2. Platforms

| Platform | Status | Notes |
|---|---|---|
| Linux desktop | Works (alpha) | Main development target. Dioxus 0.7.10 WebView, GStreamer calls, Wayland/X11 screen capture. |
| Android | Partial | Cross-compilation target and CI build job exist. No runtime validation, no media backend, no push delivery, no Keystore, no foreground service. |
| Web (WASM) | Not started | Target remains in scope. `xmpp-core` currently fails to compile for wasm and there is no WebSocket/BOSH transport or WASM storage adapter. |
| macOS | Untested | Expected to build; no CI job or manual validation. |
| Windows | Untested | Expected to build; no CI job or manual validation. |
| iOS | Not started | Target exists in the toolchain only. |

## 3. Feature status

### Accounts and connection

| Feature | Status | Notes |
|---|---|---|
| Login, TLS, STARTTLS, direct TLS | Works | TLS verification, CA file, client certificate/key, proxy settings |
| SASL mechanisms | Works | SCRAM-SHA-256/1, PLAIN, ANONYMOUS |
| Reconnect and backoff | Works | Configurable attempts; manual disconnect |
| Stream management | Works | XEP-0198 enable, ack, resume, retransmit |
| Multi-account add/remove/switch | Works (sequential) | One active connection; switching reconnects. No simultaneous sessions |
| In-band registration | Works | XEP-0077 |
| Ad-hoc commands / gateways | Partial | Discover and execute; multi-step command sessions not exposed |
| Credential storage | **Gap** | Plaintext JSON in the SQLite `settings` table; no keyring/Keystore |

### Contacts and identity

| Feature | Status | Notes |
|---|---|---|
| Roster, groups, subscriptions | Works | Approve/deny, pushes, revoke keeps the contact |
| Blocking | Works | XEP-0191 block/unblock, pushes, blocklist retrieval |
| vCard-temp and vCard4 | Works | Read-merge-write; profile editor writes both |
| Avatars | Works | XEP-0084 PEP avatar; XEP-0153 vCard avatar is not implemented |
| Presence menu | Works | Online/away/DND/XA, status, priority |
| PEP activity | Works | Tune, mood, activity, location, vCard4 |
| Entity capabilities | Partial | Sends hashed `<c/>`, stores contact caps; no caps cache or caps-driven disco |
| Last activity | Works | XEP-0012 |
| Consistent colours | Works | XEP-0392 |
| Shared media | Not started | Contact drawer does not aggregate shared media |

### 1:1 chat

| Feature | Status | Notes |
|---|---|---|
| Send/receive/persist/paginate | Works | MAM `max`/`before`, RSM cursor, archive-complete |
| Receipts, markers, chat states | Works | XEP-0184/0333/0085 |
| Replies, edits, retractions, reactions | Works | XEP-0461/0308/0424/0444 |
| Replies with quoted context | Works | Reply metadata is rendered |
| Mentions, forwarding, `/me`, attention | Works | XEP-0372/0297/0224 |
| Attachments | Works | XEP-0363 upload, OOB, file metadata, inline image/video/audio |
| Emoji, custom emoji, stickers | Works | Noto SVG, packs, XEP-0231 BOB, reactions picker |
| GIF/Tenor | Not started | Deliberately deferred |
| Link previews | Not started | No cache or privacy controls |
| Drafts, pin, mute, archive, clear | Works | Stored in SQLite settings/JSON |
| Global search | Partial | Message FTS plus in-memory contacts/rooms; no posts/spaces/stories index |
| OMEMO | Works (1:1) | XEP-0384 device lists, bundles, trust, sessions, persistence |

### Rooms (MUC)

| Feature | Status | Notes |
|---|---|---|
| Join/leave/create, subjects | Works | |
| Bookmarks and autojoin | Works | XEP-0402 and legacy `storage:bookmarks` |
| Configuration and destroy | Works | Owner/admin forms |
| Affiliations, invites, private messages | Works | Kick/ban and role/affiliation changes |
| MAM, reactions, edits, retractions, replies | Works | |
| Rejoin after reconnect | Works | Rooms are marked disconnected and rejoined on the next stream |
| Self-ping, message moderation | Not started | XEP-0410, XEP-0425 |
| Room discovery/search | Not started | XEP-0433 |
| MUC OMEMO | Not started | 1:1 OMEMO only |

### Calls and voice

| Feature | Status | Notes |
|---|---|---|
| 1:1 audio/video | Works (desktop) | Jingle, ICE, DTLS-SRTP, XEP-0353, camera switch, mute, call history |
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
| Space rooms, directories, members, affiliations | Works | Create/edit/delete, notify/pin |
| Space room updates | Works | Partial pubsub notifications are merged; refreshed on connect |
| Voice rooms inside spaces | Works (desktop) | Normal MUC rooms with a `voice` bookmark marker |
| Microblog feed | Partial | XEP-0277 publish/fetch of the user's own posts |
| Communities, explore, tags, comments, likes | Not started | XEP-0472/0496/0497/0499 are target work |
| Stories | Not started | XEP-0501; no module or UI |
| Public blog/community/space/story pages | Not started | Requires the Host crate |

### Notifications, settings, UI

| Feature | Status | Notes |
|---|---|---|
| Notification events | Partial | App emits payloads; platform delivery is not implemented |
| Desktop notifications | **Gap** | `DesktopNotifications::show_notification` only logs |
| Tray, autostart, single instance | Not started | `SystemTray` and background service are stubs |
| Android push (FCM/UnifiedPush) | Not started | XEP-0357 registration state exists; no delivery |
| Themes | Works | Dark, light, silver, butter |
| Settings | Partial | Theme, timestamps, notifications; language/font size/compact/animate are stored but unused |
| i18n | Not started | No translation layer |
| Accessibility, keyboard shortcuts | Not started | No dedicated work |
| Public Host, reporting, moderation, admin | Not started | ADR-009 describes the target; no `crates/host` |
| Data export/import CLI | Not started | Commands print a message only |

## 4. XEP status

This is the current state, not the target. `docs/PLAN.md` lists the target set.

### Core and connection

| XEP / RFC | Status | Notes |
|---|---|---|
| RFC 6120 / 6121 / 6122 / 7590 | Works | TCP, STARTTLS, direct TLS, SASL, roster, presence |
| XEP-0030 Service Discovery | Works | `disco#items`, `disco#info` |
| XEP-0115 Entity Capabilities | Partial | Sends hashed `<c/>`; no feature cache |
| XEP-0198 Stream Management | Works | Enable, ack, resume, retransmit |
| XEP-0199 Ping | Works | Both directions, pong timeout |
| XEP-0203 Delayed Delivery | Works | Parsed for MAM and live delays |
| XEP-0215 External Service Discovery | Works | STUN/TURN |
| XEP-0352 Client State Indication | Works | Active/inactive, restored after reconnect |
| XEP-0368 Direct TLS | Partial | Port 5223 works; no SRV `_xmpps-client._tcp` lookup |
| XEP-0388 SASL2 / XEP-0390 | Not started | |

### Identity and contacts

| XEP | Status | Notes |
|---|---|---|
| XEP-0012 Last Activity | Works | |
| XEP-0050 Ad-Hoc Commands | Partial | Execute only; no multi-step sessions |
| XEP-0054 vCard-temp | Works | Read-merge-write |
| XEP-0077 In-Band Registration | Works | |
| XEP-0080 User Location | Works | PEP |
| XEP-0084 User Avatar | Works | Publish/fetch/SHA-1 cache |
| XEP-0100 Gateway Interaction | Works | Data-form and legacy registration |
| XEP-0107 / 0108 / 0118 | Works | Tune, activity, mood |
| XEP-0153 vCard-Based Avatars | Not started | |
| XEP-0157 Server Contact Discovery | Works | |
| XEP-0172 User Nickname | Works | Carried in presence |
| XEP-0191 Blocking Command | Works | |
| XEP-0292 vCard4 | Works | Field-level merge |
| XEP-0319 Last User Interaction | Not started | |
| XEP-0392 Consistent Colour Generation | Works | |
| XEP-0402 PEP Native Bookmarks | Works | Autojoin |

### Messaging and rooms

| XEP | Status | Notes |
|---|---|---|
| XEP-0004 Data Forms | Works | Generic parse/submit |
| XEP-0059 RSM | Partial | `max` and `before` for MAM |
| XEP-0085 Chat States | Works | |
| XEP-0184 Receipts | Works | |
| XEP-0201 Message Threads | Not started | `thread` field exists but is not populated |
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
| XEP-0380 Explicit Message Encryption | Partial | Outbound `<encryption>`; inbound not parsed |
| XEP-0393 Message Styling | Works | Emphasis, strikethrough, monospace, links |
| XEP-0410 MUC Self-Ping | Not started | |
| XEP-0424 Message Retraction | Works | |
| XEP-0425 Message Moderation | Not started | |
| XEP-0428 Fallback Indication | Not started | |
| XEP-0433 Extended Channel Search | Not started | |
| XEP-0444 Message Reactions | Works | |
| XEP-0461 Message Replies | Works | |
| XEP-0469 Bookmark Pinning | Not started | Spaces pinning is a separate model |

### PubSub, social, Spaces, Stories

| XEP | Status | Notes |
|---|---|---|
| XEP-0060 Publish-Subscribe | Partial | Item fetch/events; subscribe is used for microblog/spaces, not a generic client |
| XEP-0163 PEP | Works | Avatars, activity, vCard4, OMEMO devices |
| XEP-0223 Persistent Private Data | Partial | Used for bookmarks private storage |
| XEP-0277 Microblogging | Partial | Own posts only |
| XEP-0330 Pubsub Subscription | Not started | |
| XEP-0402 PEP Native Bookmarks | Works | |
| XEP-0462 PubSub Type Filtering | Not started | Spaces filters after `disco#info` |
| XEP-0465 Pubsub Public Subscriptions | Not started | |
| XEP-0472 Social Feed | Partial | Microblog subset |
| XEP-0496 Pubsub Node Relationships | Not started | |
| XEP-0497 Pubsub Extended Subscriptions | Not started | |
| XEP-0499 Pubsub Extended Discovery | Not started | |
| XEP-0501 Stories | Not started | |
| XEP-0503 Server-side Spaces | Partial | Discovery/create/join/rooms/members |

### Encryption and calls

| XEP | Status | Notes |
|---|---|---|
| XEP-0384 OMEMO | Partial | 1:1 only; MUC OMEMO absent |
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

- No schema version table or forward-only migrations; the schema is created
  with `CREATE TABLE IF NOT EXISTS`.
- FTS exists for messages and MUC messages only. Posts, contacts, rooms,
  spaces and stories are not indexed.
- Drafts, chat preferences, credentials and OMEMO state are JSON blobs in
  `settings`, not relational rows.
- **Credentials and OMEMO private state are stored as plaintext JSON.** There
  is no keyring or Keystore integration. This contradicts ADR-006 and must be
  fixed before a release. See §6.

## 6. Known gaps and risks

### Security

1. **Plaintext secrets.** `credentials:<jid>`, the active account and the OMEMO
   store are stored unencrypted in the SQLite `settings` table. ADR-006 says
   OS keyring/Keystore; that is not implemented.
2. **Notification payloads are not delivered.** The desktop and mobile
   `NotificationService` implementations only log.
3. **No database encryption.** SQLCipher and keyring-backed secrets are both
   open decisions.
4. **Log redaction** is not systematically tested. Normal-level logs avoid
   message bodies, but this needs an audit before release.

### CI and release engineering

5. The `check` and `test` jobs do not install the GTK/WebKit/GStreamer system
   packages that `ui`/`media` need, so they are not expected to pass on a stock
   `ubuntu-latest` runner.
6. The `build-web` job builds a target that currently fails in `aws-lc-sys`.
   Web remains a target, but it is not buildable today.
7. The Android job only builds. There is no emulator or device smoke test, and
   no Android media backend.
8. CI does not run the Prosody or GStreamer integration suites; they require
   services and are opt-in.

### Architecture

9. The `AppService`/`AccountManager` boundary from ADR-008 is not implemented.
   The UI still owns connection lifecycle and calls `XmppClient` directly.
   Treat this as a target refactor, not as the current design.
10. One connection is active at a time. Multi-account switching reconnects.
11. There is no `crates/host`. Public pages, share links, reporting, moderation
    and administration are entirely unimplemented.
12. Group camera video is not implemented; voice channels are audio plus screen
    share.
13. i18n and accessibility work has not started.

### Product scope still open

14. Stories (XEP-0501) and communities/explore are not implemented.
15. Link previews and GIF/Tenor are not implemented.
16. MUC OMEMO, XEP-0410, XEP-0425, XEP-0433 and XEP-0469 are missing.
17. Desktop notifications, tray, autostart and packaging are missing.
18. Android push, foreground service and Keystore are missing.

## 7. Test inventory

| Suite | Count | How to run |
|---|---|---|
| `xmpp-core` unit tests | 141 | `cargo test -p xmpp-core --lib` |
| `storage` unit tests | 19 | `cargo test -p storage --lib` |
| `media` unit tests | 9 | `cargo test -p media --lib` |
| `ui` unit tests | 7 | `cargo test -p ui --lib` |
| Golden stanzas | 27 fixtures, 2 tests | `cargo test -p xmpp-core --test golden` |
| Malformed/fuzz | 2 tests | `cargo test -p xmpp-core --test malformed` |
| Prosody live | 7 tests | `JABU_TEST_XMPP=1 cargo test -p xmpp-core --test prosody_smoke -- --test-threads=1` |
| GStreamer/SFU live | 8 tests, 7 ignored | `JABU_TEST_XMPP=1 cargo test -p media --test conference_live -- --ignored --nocapture` |
| UI / app / platform | 0 | No UI tests |

The Prosody suite covers login, direct TLS, proxy, IQ routing, MAM, blocking,
room create/configure/destroy, room join, reconnect/rejoin, upload, registration,
vCard merge, last activity, spaces refresh and space-room notifications.

## 8. How to update this file

- Change the status only after a test or a documented manual run.
- Add the commit hash and date at the top.
- Keep `README.md`, `docs/PLAN.md` and `AGENTS.md` free of duplicated status;
  they should link here.
- If a feature moves from Partial to Works, add the test that proves it.
