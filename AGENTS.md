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

## Graphify (Code Knowledge Graph)

Graphify indexes the codebase into a knowledge graph so agents query structure instead of reading files, reducing token consumption by ~30x per query.

```bash
# Install graphify (one-time)
npm install -g graphifyy --prefix ~/.local
export PATH="$HOME/.local/bin:$PATH"

# Build/refresh the graph (run after significant code changes)
graphify extract . --scope all --out graphify-out \
  --exclude '**/target/**' \
  --exclude '**/.cargo/**' \
  --exclude '**/.git/**' \
  --exclude '**/node_modules/**' \
  --exclude '**/vendor/**' \
  --exclude '**/prosody/**' \
  --exclude '**/.graphify/**' \
  --exclude '*.md' \
  --exclude '*.toml' \
  --exclude '*.json' \
  --exclude '*.snap' \
  --exclude '*.cfg' \
  --exclude '*.html' \
  --exclude 'LICENSE' \
  --exclude 'docker-compose.yml' \
  --exclude 'Dockerfile' \
  --exclude '.envrc'

# Measure token savings
graphify benchmark graphify-out/.graphify/graph.json

# Query the graph (example)
graphify query graphify-out/.graphify/graph.json "what is the main entry point"
```

The graph is stored in `graphify-out/.graphify/graph.json` (gitignored). Re-run `graphify extract` after merging branches or making structural changes.

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
