# AGENTS.md

# Jabu — agent and contributor guide

This document tells agents how the repository actually works today. It is not a
status page: **`docs/STATUS.md` is the single source of truth** for what is
implemented and tested. `README.md` is the entry point and `docs/PLAN.md` is the
forward-looking plan. When behaviour changes, update STATUS first and make the
other documents link to it.

## Project overview

Jabu is a clean-room, client-side XMPP client written in Rust with
Dioxus 0.7.10. Linux desktop is the current usable target. Android and the web
remain targets but are not usable yet. The goal is full Movim parity without
copying Movim's AGPL code, templates, translations or assets.

## Repository layout

```
crates/
  xmpp-core/    Protocol engine. No Dioxus dependency.
  storage/      SQLite + FTS5 system of record; Redb legacy import only.
  media/        GStreamer webrtcbin, Jingle/SDP, Galène and conference voice.
  ui/           Dioxus components and ClientState.
  platform/     Desktop/web/mobile abstractions.
  app/          Composition root, configuration, CLI, runtime.
  vendor/       Patched prose/xso crates. Not first-party code.
dev/
  reference-testbed/  Movim + Galener + Galène reference environment.
prosody/        Local Prosody fixture (alice/bob/admin, MUC, MAM, upload, PEP).
docs/
  STATUS.md     What works, XEP matrix, gaps, test inventory.
  PLAN.md       Milestones and remaining work.
  adr/          Architecture decision records.
```

`crates/host` is planned but does not exist.

## Current architectural reality

- `xmpp-core` has no Dioxus dependency and is headless-testable.
- `ui` does **not** go through an `AppService` today. Components call
  `XmppClient` and its managers directly and own the connection lifecycle.
  ADR-008's service boundary is a target, not the current design. Do not write
  docs or code that pretend the boundary already exists.
- One account connection is active at a time. Switching accounts reconnects.
- State shared across threads uses `Arc<RwLock<T>>` (`parking_lot` for sync,
  `tokio::sync` for async). Dioxus `Signal<T>` is not `Send`.
- `storage` is SQLite. Schema changes go through the forward-only migration
  runner in `crates/storage/src/sqlite_storage.rs`; the database is stamped with
  `PRAGMA user_version` and the current version is 2.
- Credentials and the OMEMO at-rest key use the OS keyring on desktop, with a
  SQLite plaintext fallback when no keyring service is available. OMEMO state
  is AES-256-GCM encrypted when a keyring is present; see STATUS §6 and
  ADR-015.

## Development commands

All builds and tests should run inside the Nix dev shell. The system toolchain
does not provide the native libraries Dioxus desktop and GStreamer need.

```bash
nix develop

cargo check --workspace
cargo test -p xmpp-core -p storage --lib
cargo fmt --all
cargo clippy --workspace

# Desktop
dx serve --platform desktop -p app
dx bundle --platform desktop -p app

# Web (currently fails in xmpp-core; see STATUS)
dx serve --platform web -p app

# Stylesheet (after adding Tailwind classes)
tailwindcss -c tailwind.config.js -i crates/ui/assets/tailwind.css \
  -o crates/ui/assets/app.css --minify
```

The pinned toolchain is Rust 1.98.1 in `rust-toolchain.toml`. Do not upgrade it
without checking clippy and the Android/wasm targets.

## Testing

Hermetic tests:

```bash
cargo test -p xmpp-core -p storage --lib
cargo test -p ui -p media -p platform --lib
cargo test -p xmpp-core --test golden --test malformed
```

Live XMPP tests (skipped unless the env var is set):

```bash
./prosody/run.sh
JABU_TEST_XMPP=1 cargo test -p xmpp-core --test prosody_smoke -- --test-threads=1
./prosody/stop.sh
```

Media and SFU tests are mostly `#[ignore]` and need the Movim/Galener testbed:

```bash
JABU_TEST_XMPP=1 cargo test -p media --test conference_live -- --ignored --nocapture
JABU_TEST_XMPP=1 cargo test -p media --test conference_client_flow -- --ignored --nocapture
```

The testbed lives in `dev/reference-testbed/`; read its README before running it.
Its Movim checkout, database, caches and logs are gitignored.

Do not add a test that silently passes when its service is missing; follow the
existing `JABU_TEST_XMPP` skip pattern.

## Code conventions

- Follow the surrounding style in each crate.
- Comments: avoid narration, but keep doc comments for public items and
  non-obvious protocol reasoning. The "no comments" rule means no
  restate-the-code comments.
- Use `thiserror` for crate errors and the crate `Result<T>` alias.
- Use `Readable`/`Writable` (imported as `ReadableExt`/`WritableExt` in 0.7) for
  Dioxus signals; `Signal::set()` takes `&mut self`.
- Use `parking_lot::RwLock` for sync contexts and `tokio::sync::RwLock` for
  async contexts.
- Keep `xmpp-core` free of UI types.
- Capability-gate experimental XEPs and degrade gracefully.
- Never log message bodies, passwords, tokens or key material at normal levels.
- Do not copy Movim code, templates, translations or assets. XEPs and observed
  behaviour only.

## Formatting and lints

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
```

CI does not use `-D warnings` while the warning backlog is cleaned up. Fix
warnings touched by your change; do not add new ones.

## Architecture decision records

See `docs/adr/`. Status matters: some are superseded and some describe targets
that are not implemented.

| ADR | Topic | Status |
|---|---|---|
| ADR-001 | Dioxus selection | Historical; version superseded by ADR-007 |
| ADR-002 | tokio-xmpp vs prose-xmpp | Accepted |
| ADR-003 | Concurrency model | Accepted |
| ADR-004 | Redb storage | Superseded by ADR-006 |
| ADR-005 | No E2EE | Superseded; OMEMO 1:1 is implemented |
| ADR-006 | SQLite + FTS5 | Accepted; credentials use the keyring, OMEMO state still pending |
| ADR-007 | Dioxus 0.7.10 | Accepted; Android runtime still unvalidated |
| ADR-008 | Headless core / AppService boundary | Target; UI boundary not enforced yet |
| ADR-009 | Optional Host/Gateway | Accepted; not started |
| ADR-010 | Call media backend | Accepted for desktop; Android pending |
| ADR-011 | Emoji assets | Accepted |
| ADR-012 | Spaces service and voice channels | Accepted; see ADR-014 for voice |
| ADR-013 | Direct Galène voice | Superseded by ADR-014 |
| ADR-014 | XMPP SFU conferences | Accepted and implemented on desktop (audio + screen share) |
| ADR-015 | OMEMO state encrypted at rest | Accepted; keyring key, plaintext fallback |

If a decision changes, add a new ADR and mark the old one superseded rather than
rewriting history. Small factual corrections to an existing ADR are fine.

## Known issues to keep in mind

- `xmpp-core` does not compile for `wasm32-unknown-unknown` (`aws-lc-sys`).
- Desktop notifications use `notify-rust`; mobile notifications are stubs that
  only log.
- The CI `check` and `test` jobs install GTK/WebKit/GStreamer dependencies, but
  the workflow has not been re-run since. `build-web` is informational and still
  fails at `xmpp-core`; only the service-backed suites remain opt-in.
- Android has no media backend, push delivery, foreground service or Keystore.
- Desktop tray/autostart/single-instance and packaging are not implemented.
- Public Host, Stories, communities, moderation/reporting and admin are not
  implemented.
- Group camera video is not implemented; voice channels are audio + screen share.
- The local `target/` can contain stale root-owned artifacts. Prefer a fresh
  `CARGO_TARGET_DIR` if cargo reports permission errors.

## Graphify (optional)

The codebase can be indexed into a knowledge graph to reduce exploration tokens.

```bash
npm install -g graphifyy --prefix ~/.local
export PATH="$HOME/.local/bin:$PATH"

graphify extract . --scope all --out graphify-out \
  --exclude '**/target/**' --exclude '**/.cargo/**' --exclude '**/.git/**' \
  --exclude '**/node_modules/**' --exclude '**/vendor/**' \
  --exclude '**/prosody/**' --exclude '**/.graphify/**' \
  --exclude '*.md' --exclude '*.toml' --exclude '*.json' \
  --exclude '*.snap' --exclude '*.cfg' --exclude '*.html' \
  --exclude 'LICENSE' --exclude 'docker-compose.yml' \
  --exclude 'Dockerfile' --exclude '.envrc'

graphify query graphify-out/.graphify/graph.json "what is the main entry point"
```

`graphify-out/` and `.graphify/` are gitignored. Refresh the graph after
structural changes.

## Documentation rules

1. `docs/STATUS.md` is the only status page. Update it when behaviour changes.
2. `README.md` stays short: overview, platform table, build/test, links.
3. `docs/PLAN.md` stays forward-looking: remaining work and exit criteria.
4. Do not duplicate XEP matrices or feature tables across files.
5. Record decisions in `docs/adr/`, not in README.
