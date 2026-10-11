# Jabu

A clean-room, client-side XMPP client implementation in Rust using Dioxus 0.7 for the UI. Targets web, desktop, and mobile platforms.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.98-blue.svg)](https://www.rust-lang.org)
[![Dioxus](https://img.shields.io/badge/Dioxus-0.7-blue.svg)](https://dioxuslabs.com)

## Status

**MVP in progress.** Desktop builds on Dioxus 0.7 and connects over XMPP with 1:1 chat, MUC rooms, roster/presence menu, MAM history, file upload, reactions/replies/edits, drafts, pinned/muted/archived chats, global search, multi-file uploads, structured file attachments, forwarding, mentions, /me actions, attention nudges, room invitations, room configuration, avatars, OMEMO 1:1 and a microblog feed. Web/WASM transport and the broader Movim parity work remain outstanding; see `docs/PLAN.md`.

## Architecture

```
crates/
  xmpp-core/    - XMPP protocol layer (tokio-xmpp, connection, roster, chat, MUC, PubSub, events, processor)
  storage/      - Local storage abstraction (SQLite + FTS5; Redb legacy import)
  ui/           - Dioxus UI components
  platform/     - Cross-platform abstraction (desktop/web/mobile)
  app/          - Main application entry point
```

### Key Design Decisions

1. **Dioxus 0.7 signals API**: Uses `Signal<T>` with `ReadableExt`/`WritableExt` traits, `set()` takes `&mut self`
2. **Thread safety**: `Arc<RwLock<T>>` used for cross-thread state (Dioxus `Signal` is not `Send`)
3. **XMPP**: Direct `tokio-xmpp` integration from xmpp-rs git repo (not prose-xmpp)
4. **Storage**: `Arc<dyn StorageBackend>` trait with SQLite + FTS5; Redb is retained only for legacy import
5. **E2EE**: OMEMO 1:1 encryption (bundles, sessions, trust, persistence); MUC encryption pending

## XEP support

Status legend: **Implemented** = works and is tested; **Partial** = the core path exists but a documented part is missing; **Planned** = designed in `docs/PLAN.md`, not implemented.

### Core and connection

| XEP / RFC | Feature | Status | Notes |
|-----------|---------|--------|-------|
| RFC 6120 | XMPP Core | Implemented | TCP, STARTTLS, direct TLS on 5223, SASL (SCRAM-SHA-256/1, PLAIN, ANONYMOUS) |
| RFC 6121 | XMPP IM | Implemented | Roster, presence, subscriptions, 1:1 messaging |
| XEP-0030 | Service Discovery | Implemented | `disco#items` and `disco#info` |
| XEP-0077 | In-Band Registration | Implemented | Form fetch, account creation, login test |
| XEP-0115 | Entity Capabilities | Partial | Sends a hashed `<c/>`; stores contact caps; no feature cache or caps-driven disco |
| XEP-0157 | Server Contact Discovery | Implemented | Abuse/admin/feedback/sales/security/support addresses |
| XEP-0198 | Stream Management | Implemented | Enable, acks, unacked queue, resume and retransmit |
| XEP-0199 | XMPP Ping | Implemented | Client and server pings, pong timeout detection |
| XEP-0215 | External Service Discovery | Implemented | STUN/TURN discovery after `extdisco:2` |
| XEP-0280 | Message Carbons | Implemented | Sent/received attribution |
| XEP-0352 | Client State Indication | Implemented | Active/inactive, restored after reconnect |
| XEP-0368 | Direct TLS | Partial | Port 5223 works; SRV `_xmpps-client._tcp` lookup pending |
| XEP-0191 | Blocking Command | Implemented | Block/unblock, pushes, blocklist retrieval and persistence |
| XEP-0050 | Ad-Hoc Commands | Partial | Discover and execute commands; multi-step session continuation not exposed |
| XEP-0100 | Gateway Interaction | Implemented | Discovers gateway components, renders data-form and legacy registration forms, registers and removes |

### Messaging and archives

| XEP / RFC | Feature | Status | Notes |
|-----------|---------|--------|-------|
| XEP-0012 | Last Activity | Implemented | Queries contacts/server; stores `last_seen` |
| XEP-0045 | Multi-User Chat | Implemented | Join/leave/create, subjects, invites, moderation, MUC MAM |
| XEP-0048 / XEP-0402 | Bookmarks | Implemented | PEP bookmarks with autojoin |
| XEP-0059 | Result Set Management | Partial | `max` and `before` for MAM |
| XEP-0060 | Publish-Subscribe | Implemented | Items fetch and event notifications |
| XEP-0085 | Chat State Notifications | Implemented | Composing/paused/active |
| XEP-0184 | Message Delivery Receipts | Implemented | Request/received |
| XEP-0203 | Delayed Delivery | Partial | Parsed for MAM timestamps |
| XEP-0249 | Direct MUC Invitations | Implemented | Invite and surface room |
| XEP-0277 | Microblogging | Implemented | PEP microblog read/post |
| XEP-0308 | Last Message Correction | Implemented | 1:1 and MUC |
| XEP-0313 | Message Archive Management | Implemented | 1:1 and MUC, `<fin>`, RSM cursor, archive-complete |
| XEP-0333 | Chat Markers | Implemented | Displayed/received/acknowledged |
| XEP-0334 | Message Processing Hints | Implemented | `store`, `no-store`, `no-copy`, `markable` |
| XEP-0359 | Unique and Stable Stanza IDs | Implemented | `origin-id` and `stanza-id`, dedup across live/MAM |
| XEP-0424 | Message Retraction | Implemented | 1:1 and MUC |
| XEP-0444 | Message Reactions | Implemented | 1:1 and MUC |
| XEP-0461 | Message Replies | Implemented | Quoted reply metadata |
| XEP-0472 | Social Feed | Partial | Microblog subset only |

### Identity, presence and media

| XEP / RFC | Feature | Status | Notes |
|-----------|---------|--------|-------|
| XEP-0054 | vCard-temp | Implemented | Fetch and read-merge-write without dropping fields |
| XEP-0080 | User Location | Implemented | Publish/store location |
| XEP-0084 | User Avatar (PEP) | Implemented | Publish, fetch, SHA-1 cache |
| XEP-0107 | User Tune | Implemented | Publish/store |
| XEP-0108 | User Activity | Implemented | Publish/store |
| XEP-0118 | User Mood | Implemented | Publish/store |
| XEP-0153 | vCard-Based Avatars | Planned | Not implemented |
| XEP-0163 | Personal Eventing Protocol | Implemented | Avatars, activity, vCard4, OMEMO devices |
| XEP-0172 | User Nickname | Implemented | Carried in presence from the vCard |
| XEP-0292 | vCard4 | Implemented | Publish, fetch, store and field-level read-merge-write; the profile editor writes both vCard-temp and vCard4 |
| XEP-0357 | Push Notifications | Partial | Registration/unregistration; platform delivery wiring pending |
| XEP-0363 | HTTP File Upload | Implemented | Native upload and slot discovery; WASM path written but untested |
| XEP-0384 | OMEMO | Partial | 1:1 encryption, sessions, trust, prekey top-up; MUC OMEMO pending |
| XEP-0392 | Consistent Colour Generation | Implemented | Deterministic identity colours |
| XEP-0393 | Message Styling | Implemented | Emphasis, strikethrough, monospace, links |

### Planned, not implemented

| XEP | Feature | Notes |
|-----|---------|-------|
| XEP-0138 | Stream Compression | Not enabled; optional |
| XEP-0166 / XEP-0167 / XEP-0176 / XEP-0320 | Jingle / WebRTC calls | Audio/video and data channels |
| XEP-0234 / XEP-0261 | Jingle File Transfer | Replaced in the current MVP by HTTP upload |
| XEP-0047 / XEP-0065 | In-band / SOCKS5 file transfer | Legacy fallback |
| XEP-0319 | Last User Interaction in Presence | Idle time |
| XEP-0410 | MUC Self-Ping | Keepalive for joined rooms |
| XEP-0425 | Message Moderation | Retract others' MUC messages |
| XEP-0433 | Extended Channel Search | Room discovery/search |
| XEP-0156 | Discovering Alternative XMPP Connection Methods | WebSocket/BOSH discovery for web |

Non-XEP work still planned: simultaneous multi-account sessions, full Movim Spaces/Stories/communities, Android emulator smoke tests and Galene-compatible call fixture.

## Platforms

| Platform | Status | Notes |
|----------|--------|-------|
| Web (WASM) | ❌ | UI compiles in principle, but XMPP transport needs a WebSocket client |
| Linux | ✅ | Builds, connects and chats; needs `xdotool` for desktop tests |
| macOS | ⚠️ | Expected to build, untested |
| Windows | ⚠️ | Expected to build, untested |
| Android | ⚠️ | Cross-target installed, untested |
| iOS | ⚠️ | Cross-target installed, untested |

## Getting Started

### Prerequisites

```bash
# Enter dev shell (provides Rust toolchain, GTK/WebKit, xdotool)
nix develop
```

All builds and tests must run inside `nix develop`; the system rustup toolchain
has a broken linker wrapper and does not provide the native libraries Dioxus
desktop needs.

### Building

```bash
# Check compilation
cargo check --workspace

# Run tests (core crates)
cargo test -p xmpp-core -p storage --lib

# Format code
cargo fmt --all

# Lint code
cargo clippy --workspace

# Serve web dev
dx serve --platform web

# Serve desktop dev
dx serve --platform desktop
```

### Configuration

On first launch with no configured JID the app shows a sign-in screen. When
"Remember this account" is checked the JID, resource and password are stored in
the local redb settings so the next launch signs in automatically.

An account can also be configured in `config.toml`:

```toml
[xmpp]
jid = "user@example.com"
password = "optional"
resource = "jabu"
```

## Roadmap / Missing Features

### High Priority

1. **WebSocket transport for web/WASM** — current `tokio-xmpp` TCP path cannot run in the browser
2. **OMEMO hardening** — MUC OMEMO and interop testing against Conversations/Movim
3. **Push delivery wiring** — platform push services (FCM/APNs) on top of the XEP-0357 registration
4. **Multi-account support** — add/remove/switch accounts with per-account settings

### Medium Priority

5. **Full RSM support** — `after`, limits and cursors outside MAM
6. **Ad-hoc commands and gateways** — XEP-0050 / XEP-0100
7. **Theme support** — light/high-contrast palettes beyond the dark default

### Low Priority

8. **Jingle / WebRTC** — audio/video calls
9. **Legacy file transfer (XEP-0065 / XEP-0047)**
10. **Compression (XEP-0138)**

## Differences from Movim (PHP Reference)

- **Client-side only**: No server component; connects to external XMPP server
- **Single-user**: Desktop/mobile app vs multi-user web app
- **OMEMO**: 1:1 encryption with persisted state, trust management and prekey top-up; MUC encryption is not implemented
- **Feed scope**: Microblog posting/reading is implemented; full Movim community/feed features are not

## Development

### Project Structure

- **xmpp-core**: Direct tokio-xmpp implementation (Connection, Roster, Chat, MUC, PubSub, Disco/Upload, microblog, EventProcessor)
- **storage**: SQLite + FTS5 with `Arc<dyn StorageBackend>`; Redb retained for legacy import
- **ui**: Dioxus 0.7 components (chat list, conversation view, contacts, rooms, feed)
- **platform**: Cross-platform abstraction (desktop/web/mobile)
- **app**: Main entry point with thread-local context for non-Send types

### Running Tests

```bash
# Core crates only (desktop needs xdotool)
cargo test -p xmpp-core -p storage --lib

# All crates (may fail on desktop due to missing system deps)
cargo test --workspace
```

### Code Style

```bash
cargo fmt --all
cargo clippy --workspace
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests and lints
5. Submit a PR

## Acknowledgments

- [Movim](https://movim.eu) - Inspiration and XMPP expertise
- [prose-im/xmpp-rs](https://github.com/prose-im/xmpp-rs.git) - XMPP library (tokio-xmpp, minidom, jid)
- [Dioxus](https://dioxuslabs.com) - Rust UI framework
- [XMPP Standards Foundation](https://xmpp.org) - Protocol specifications
