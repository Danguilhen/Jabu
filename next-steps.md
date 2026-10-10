# Next Steps - Jabu

## Overview

This document tracks missing features, known issues, and differences between Jabu and the reference Movim implementation (PHP/XMPP web client).

## Environment Issues

### Clippy

- **Status**: Fixed
- **Issue**: `cargo clippy` failed due to rustc version mismatch in nix-shell environment (rustc 1.98.1 from nix vs clippy 0.1.99 from rustup/stable)
- **Fix**: Set `channel = "1.98.1"` in `rust-toolchain.toml` and set rustup default toolchain to `1.98.1-x86_64-unknown-linux-gnu`, ensuring both rustc and clippy come from the same rustup toolchain
- **Result**: `cargo clippy --workspace` passes with warnings only (no errors)

### Desktop Platform Tests

- **Status**: Blocked
- **Issue**: Missing system library `libxdo` for desktop platform tests
- **Details**: Linking fails with `cannot find -lxdo` when building desktop-specific crates (ui, platform)
- **Workaround**: Core crates (xmpp-core, storage) test successfully; desktop platform tests are skipped
- **Fix needed**: Install `libxdo` development package or add it to the nix flake dependencies

## Missing Features vs Movim

### XMPP Core Protocol

| Feature | Status | Notes |
|---------|--------|-------|
| XMPP Connection (tokio-xmpp) | ✅ Done | TCP connector, SimpleClient integration |
| Roster Management | ✅ Done | Add/remove contacts, subscription handling |
| 1:1 Chat | ✅ Done | Message send/receive, unread count |
| MUC (Group Chat) | ✅ Done | Join/leave rooms, occupant management |
| PubSub | ⚠️ Partial | Structure exists, no event handling |
| EventProcessor | ✅ Done | Processes connection events (connected, auth, disconnected, stanzas) |
| MAM (Message Archive Management) | ❌ Reverted | Caused build issues, removed |
| XEP-0184 Message Receipts | ❌ Missing | Not implemented |
| XEP-0085 Chat State Notifications | ❌ Missing | Structure exists, not wired |
| XEP-0444 Message Reactions | ❌ Missing | Data structures exist, no UI/logic |
| XEP-0199 Keepalive | ✅ Done | Whitespace keepalive in connection module |
| Stream Management | ❌ Missing | Not implemented |
| Compression | ❌ Missing | Not implemented |
| TLS/SSL | ⚠️ Partial | TlsConfig exists but not wired to connection |
| Proxy Support | ❌ Missing | ProxyConfig exists but not implemented |

### Storage

| Feature | Status | Notes |
|---------|--------|-------|
| Redb Backend | ✅ Done | Local embedded database |
| Accounts | ✅ Done | Save/load/list/delete |
| Contacts/Roster | ✅ Done | Save/load with subscription state |
| Chats | ✅ Done | Save/load with unread counts |
| Messages | ✅ Done | Save/load with pagination |
| MUC Rooms | ✅ Done | Save/load room config |
| MUC Messages | ✅ Done | Save/load with sender info |
| Settings | ✅ Done | Key-value storage |
| SQLite Backend | ❌ Missing | Code exists but optional, not wired |
| IndexedDB (WASM) | ❌ Missing | Code exists but optional, not wired |
| Data Export/Import | ❌ Missing | CLI commands exist but not implemented |

### UI (Dioxus 0.6)

| Feature | Status | Notes |
|---------|--------|-------|
| ClientState Signals | ✅ Done | Config, client, connection_state signals |
| Connection State Display | ✅ Done | ConnectionState enum with is_connected/is_authenticated |
| Full Chat UI | ❌ Missing | No chat windows, message list, input |
| Roster UI | ❌ Missing | No contact list, presence display |
| MUC UI | ❌ Missing | No room list, occupant list, chat |
| Settings UI | ❌ Missing | Config exists but no UI |
| Themes | ❌ Missing | "dark" default only, no switching |
| Notifications | ❌ Missing | Desktop/sound/push config exists, not implemented |

### Platform

| Feature | Status | Notes |
|---------|--------|-------|
| PlatformContext | ✅ Done | Abstract platform initialization |
| Desktop Platform | ✅ Done | Basic initialization |
| Web Platform | ⚠️ Partial | Gated behind cfg, minimal implementation |
| Mobile Platform | ⚠️ Partial | Gated behind cfg, minimal implementation |
| Notifications | ❌ Missing | Platform notifications not wired |
| Storage Directory | ✅ Done | Cross-platform data dir resolution |

### App/CLI

| Feature | Status | Notes |
|---------|--------|-------|
| CLI (clap) | ✅ Done | Run, Version, Account, Data subcommands |
| Config Loading | ✅ Done | TOML + env prefix (JABU_) |
| JID/Password Override | ✅ Done | CLI args override config |
| Account Management | ❌ Partial | List/add/remove stubs only |
| Data Import/Export | ❌ Partial | Stub commands only |
| Auto-reconnect | ⚠️ Partial | Config exists, reconnect logic in connection module |

## Differences from Movim (PHP Reference)

1. **Language/Stack**: Rust + Dioxus + tokio-xmpp vs PHP + Symfony + prosody
2. **Architecture**: Client-side only, local-first vs Server+Client web app
3. **XMPP Library**: tokio-xmpp directly vs prosody/prose-xmpp abstractions
4. **Storage**: Local Redb/SQLite vs Server-side database (MySQL/PostgreSQL)
5. **UI Framework**: Dioxus 0.6 (Rust) vs Symfony/Twig (PHP)
6. **E2EE**: Omemo removed entirely vs Optional OMEMO in Movim
7. **Multi-user**: Single-user desktop app vs Multi-user web app
8. **PubSub**: Not implemented vs Blogging/PEP in Movim
9. **File Transfer**: Not implemented vs File sharing in Movim
10. **VCard/Avatar**: Not implemented vs Profile pictures in Movim

## Known Limitations

1. No server-side component - client connects to external XMPP server only
2. No file transfer support (XEP-0065, XEP-0363)
3. No avatar/vCard support (XEP-0153)
4. No MUC configuration/room creation UI
5. No message editing/retraction UI (data structures exist)
6. No push notifications
7. No desktop notifications
8. No sound notifications
9. No contact groups UI
10. No search functionality

## Build & Test

```bash
# Check compilation
cargo check --workspace

# Run tests (core crates only, desktop needs libxdo)
cargo test -p xmpp-core -p storage --lib

# Format
cargo fmt --all

# Clippy (blocked by toolchain mismatch)
cargo clippy --workspace
```

## Priority Order for Next Work

1. Fix clippy toolchain mismatch (pin rust-toolchain.toml)
2. Add libxdo to nix flake for desktop tests
3. Implement basic chat UI (message list, input, send)
4. Implement roster UI (contact list, presence)
5. Add MAM sync (re-add with fixed minidom API usage)
6. Implement PubSub event handling
7. Add message receipts (XEP-0184)
8. Add chat state notifications (XEP-0085)
