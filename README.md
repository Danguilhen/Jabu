# Jabu

A clean-room, client-side XMPP client implementation in Rust using Dioxus 0.6 for the UI. Targets web, desktop, and mobile platforms.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.98-blue.svg)](https://www.rust-lang.org)
[![Dioxus](https://img.shields.io/badge/Dioxus-0.6-blue.svg)](https://dioxuslabs.com)

## Status

**MVP in progress.** Core crates compile and basic XMPP connection works. UI, full chat, and many XEPs are not yet implemented.

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
5. **No E2EE**: Omemo/e2ee removed from MVP

## Supported XEPs

| XEP | Feature | Status |
|-----|---------|--------|
| RFC 6120 | XMPP Core | ✅ |
| RFC 6121 | XMPP IM | ✅ |
| XEP-0045 | Multi-User Chat | ⚠️ Partial (structures exist, no UI) |
| XEP-0060 | Publish-Subscribe | ⚠️ Partial (structures exist, no event handling) |
| XEP-0163 | PEP | ❌ Not implemented |
| XEP-0184 | Message Delivery Receipts | ❌ Not implemented |
| XEP-0198 | Stream Management | ❌ Not implemented |
| XEP-0280 | Message Carbons | ❌ Not implemented |
| XEP-0313 | Message Archive Management | ❌ Not implemented |
| XEP-0357 | Push Notifications | ❌ Not implemented |
| XEP-0384 | OMEMO Encryption | ❌ Not in MVP |
| XEP-0393 | Message Styling | ❌ Not implemented |
| XEP-0424 | Message Retraction | ❌ Not implemented |
| XEP-0444 | Message Reactions | ❌ Not implemented |

## Platforms

| Platform | Status | Notes |
|----------|--------|-------|
| Web (WASM) | ✅ | Compiles, basic UI |
| Linux | ✅ | Compiles, requires xdotool for tests |
| macOS | ⚠️ | Compiles, untested |
| Windows | ⚠️ | Compiles, untested |
| Android | ⚠️ | Compiles, untested |
| iOS | ⚠️ | Compiles, untested |

## Getting Started

### Prerequisites

```bash
# Enter dev shell (provides Rust toolchain, GTK/WebKit, xdotool)
nix develop
```

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

1. **Fix clippy toolchain** — Pin `rust-toolchain.toml` to `1.98.1`, set rustup default toolchain to match nix-shell rustc (done)
2. **Add `libxdo` for desktop tests** — Added `xdotool` to `flake.nix` and `shell.nix` (done)
3. **Basic chat UI** — Message list, input field, send button
4. **Roster UI** — Contact list with presence indicators
5. **MAM sync** — Message Archive Management (re-added with correct minidom API)

### Medium Priority

6. **Message receipts (XEP-0184)**
7. **Chat state notifications (XEP-0085)**
8. **PubSub event handling** — Structure exists, no processing
9. **Message retraction (XEP-0424)** — Data structure exists, no logic
10. **Message reactions (XEP-0444)** — Data structure exists, no logic

### Low Priority

11. **E2EE / OMEMO** — Deferred post-MVP
12. **File transfer (XEP-0065, XEP-0363)**
13. **Avatar / vCard (XEP-0153)**
14. **Push notifications (XEP-0357)**
15. **Multi-user chat UI** — Room list, occupant list, room chat

## Differences from Movim (PHP Reference)

- **Client-side only**: No server component; connects to external XMPP server
- **Single-user**: Desktop/mobile app vs multi-user web app
- **No E2EE**: OMEMO removed from MVP
- **No file transfer**: Not yet implemented
- **No avatar/vCard**: Not yet implemented
- **No PubSub blogging**: Structure exists, no event handling

## Development

### Project Structure

- **xmpp-core**: Direct tokio-xmpp implementation (Connection, Roster, Chat, MUC, PubSub, EventProcessor)
- **storage**: Redb backend with `Arc<dyn StorageBackend>` trait
- **ui**: Dioxus 0.6 components (minimal, not yet functional)
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
