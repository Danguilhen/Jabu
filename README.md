# Jabu

A clean-room, client-side XMPP client implementation in Rust using Dioxus 0.6 for the UI. Targets web, desktop, and mobile platforms.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.98-blue.svg)](https://www.rust-lang.org)
[![Dioxus](https://img.shields.io/badge/Dioxus-0.6-blue.svg)](https://dioxuslabs.com)

## Status

**MVP largely in place.** Desktop builds and connects over XMPP with live 1:1 chat, MUC rooms, roster/presence, MAM history, file upload, reactions/replies/edits, avatars and a microblog feed. Web/WASM transport and end-to-end encryption are still outstanding.

## Architecture

```
crates/
  xmpp-core/    - XMPP protocol layer (tokio-xmpp, connection, roster, chat, MUC, PubSub, events, processor)
  storage/      - Local storage abstraction (redb backend)
  ui/           - Dioxus UI components
  platform/     - Cross-platform abstraction (desktop/web/mobile)
  app/          - Main application entry point
```

### Key Design Decisions

1. **Dioxus 0.6 signals API**: Uses `Signal<T>` with `Readable`/`Writable` traits, `set()` takes `&mut self`
2. **Thread safety**: `Arc<RwLock<T>>` used for cross-thread state (Dioxus `Signal` is not `Send`)
3. **XMPP**: Direct `tokio-xmpp` integration from xmpp-rs git repo (not prose-xmpp)
4. **Storage**: `Arc<dyn StorageBackend>` trait with redb implementation
5. **E2EE**: OMEMO groundwork landed (bundles, device lists, payload AES-GCM); session backend pending

## Supported XEPs

| XEP | Feature | Status |
|-----|---------|--------|
| RFC 6120 | XMPP Core (TCP, STARTTLS/SRV, SASL) | ✅ |
| RFC 6121 | XMPP IM (roster, presence, messaging) | ✅ |
| XEP-0030 | Service Discovery | ✅ |
| XEP-0045 | Multi-User Chat | ✅ Join/leave/create, subjects, invites, moderation, archive |
| XEP-0054 | vCard-temp (avatars) | ✅ |
| XEP-0059 | Result Set Management | ⚠️ Partial (`max` only) |
| XEP-0060 | Publish-Subscribe | ✅ Items fetch and event notifications |
| XEP-0085 | Chat State Notifications | ✅ |
| XEP-0163 | PEP | ⚠️ Used for the microblog node; no caps discovery |
| XEP-0184 | Message Delivery Receipts | ✅ |
| XEP-0191 | Blocking Command | ✅ |
| XEP-0198 | Stream Management | ✅ Enable + inbound/outbound acks; resume pending |
| XEP-0203 | Delayed Delivery | ⚠️ Parsed for MAM timestamps |
| XEP-0249 | Direct MUC Invitations | ✅ |
| XEP-0277 | Microblogging | ✅ |
| XEP-0280 | Message Carbons | ✅ |
| XEP-0308 | Last Message Correction | ✅ |
| XEP-0313 | Message Archive Management | ✅ 1:1 and MUC |
| XEP-0333 | Chat Markers | ✅ |
| XEP-0357 | Push Notifications | ✅ Register/unregister; delivery wiring pending |
| XEP-0363 | HTTP File Upload | ✅ (native; wasm code path written) |
| XEP-0384 | OMEMO Encryption | ✅ 1:1 encryption, persisted sessions, device trust UI, prekey top-up |
| XEP-0393 | Message Styling | ❌ Not implemented |
| XEP-0424 | Message Retraction | ✅ |
| XEP-0444 | Message Reactions | ✅ |
| XEP-0461 | Message Replies | ✅ |
| XEP-0472 | Social Feed | ⚠️ Microblog subset |

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

See `config.toml` for XMPP account settings:

```toml
[xmpp]
jid = "user@example.com"
password = "optional"
resource = "jabu"
```

## Roadmap / Missing Features

### High Priority

1. **Stream resume (XEP-0198)** — reconnect with `previd` and replay unacked stanzas
2. **WebSocket transport for web/WASM** — current `tokio-xmpp` TCP path cannot run in the browser
3. **OMEMO hardening** — interop testing against Conversations/Movim; post-quantum later
4. **Push delivery wiring** — connect XEP-0357 registrations to platform notifications

### Medium Priority

5. **MUC occupant UI** — member list with roles/affiliations, kick/ban actions
6. **Login / account settings screen** — first-run JID and password entry with stored credentials
7. **Notification wiring** — dispatch XMPP events to platform notifications
8. **History pagination** — RSM `before`/`after` for scrolling older messages
9. **Message styling (XEP-0393)** — inline emphasis, monospace and links

### Low Priority

10. **Jingle / WebRTC** — audio/video calls
11. **Legacy file transfer (XEP-0065 / XEP-0047)**
12. **Multi-account support**
13. **Compression (XEP-0138)**

## Differences from Movim (PHP Reference)

- **Client-side only**: No server component; connects to external XMPP server
- **Single-user**: Desktop/mobile app vs multi-user web app
- **OMEMO**: 1:1 encryption with persisted state, trust management and prekey top-up; MUC encryption is not implemented
- **Feed scope**: Microblog posting/reading is implemented; full Movim community/feed features are not

## Development

### Project Structure

- **xmpp-core**: Direct tokio-xmpp implementation (Connection, Roster, Chat, MUC, PubSub, Disco/Upload, microblog, EventProcessor)
- **storage**: Redb backend with `Arc<dyn StorageBackend>` trait
- **ui**: Dioxus 0.6 components (chat list, conversation view, contacts, rooms, feed)
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
