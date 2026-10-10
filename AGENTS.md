# AGENTS.md

# Jabu - Agent Guidelines

This document provides guidance for agents (AI assistants) working on the Jabu project.

## Project Overview

Jabu is a clean-room, client-side XMPP client implementation in Rust using Dioxus 0.6 for the UI. It targets web, desktop, and mobile platforms.

## Architecture

### Crate Structure

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

## Development Commands

```bash
# Enter dev shell
nix develop

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

## Coding Conventions

- **No comments in code** unless explicitly asked
- Follow existing code style in each crate
- Use `Readable`/`Writable` traits for Dioxus signals
- `Signal::set()` takes `&mut self`
- Wrap non-Send types in `Arc<RwLock<T>>` for cross-thread access
- Use `parking_lot::RwLock` for sync contexts, `tokio::sync::RwLock` for async contexts
- Error types use `thiserror`; result type is `crate::Result<T>`

## Architecture Decision Records (ADRs)

See `docs/adr/` for architectural decisions:
- `ADR-001-ui-framework.md` - Dioxus 0.6 selection
- `ADR-002-xmpp-library.md` - tokio-xmpp vs prose-xmpp
- `ADR-003-concurrency-model.md` - Arc<RwLock<T>> for cross-thread state
- `ADR-004-storage-backend.md` - redb local storage
- `ADR-005-e2ee-scope.md` - No E2EE in MVP

## Testing

- Core crates (`xmpp-core`, `storage`) have unit tests
- Desktop platform tests require `libxdo` (provided by `xdotool` in nix shell)
- Run `cargo test -p xmpp-core -p storage --lib` for core tests
- Dioxus Signal-based tests are marked `#[ignore]` (require Dioxus context)

## Known Issues

- Clippy requires matching rustc/clippy toolchain versions (pinned to 1.98.1 in `rust-toolchain.toml`)
- Desktop platform linking requires `xdotool` package in nix shell
- Some Dioxus Signal tests are ignored (require Dioxus runtime context)
