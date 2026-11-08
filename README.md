# Jabu

A clean-room, client-side XMPP client written in Rust with Dioxus 0.7. The
long-term goal is full Movim parity across desktop, Android and the web. The
current usable target is Linux desktop.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.98-blue.svg)](https://www.rust-lang.org)
[![Dioxus](https://img.shields.io/badge/Dioxus-0.7-blue.svg)](https://dioxuslabs.com)

## Status

**Alpha, desktop-first.** Linux desktop builds, connects over XMPP and covers
1:1 chat, MUC rooms, contacts/roster, file uploads, reactions/replies/edits,
drafts and pins, global search, OMEMO 1:1, 1:1 audio/video calls, Spaces,
Galener/ProtoXEP voice channels, room moderation, public room search, bookmark
pinning, on-demand link previews, native desktop notifications, OS-keyring
credentials, encrypted OMEMO state and a microblog feed.

Android builds, launches and logs in against the local Prosody fixture on an
emulator, but has no media backend, push delivery or Keystore integration. The
web remains in scope: `xmpp-core` does not currently compile for wasm. An
optional Host (`jabu host`, or the standalone `jabu-host` binary) serves
public blog, contact, community, Space and Story pages with Atom/RSS feeds,
share links, rate limiting, report intake, a moderation queue, role-based
admin sessions, editable settings/branding, retention and backup downloads.
The app submits reports with `[reporting] host` and opens `jabu://` links; the
Android console is not implemented. See [docs/HOST.md](docs/HOST.md).

The authoritative feature and XEP matrix is **[docs/STATUS.md](docs/STATUS.md)**.
The remaining work is **[docs/PLAN.md](docs/PLAN.md)**; scope and the target
protocol set are in [ADR-017](docs/adr/ADR-017-parity-scope-and-release-order.md)
and [ADR-018](docs/adr/ADR-018-target-protocol-coverage.md).

## Platforms

| Platform | Status | Notes |
|---|---|---|
| Linux | Usable (alpha) | Dioxus desktop, GStreamer calls, X11/Wayland screen capture |
| Android | Partial | Debug APK builds and launches on an x86_64 API 34 emulator; no media, push or Keystore yet |
| Web (WASM) | Not buildable | Target remains; needs a WebSocket transport and a wasm-safe crypto/TLS path |
| macOS | Untested | Expected to build |
| Windows | Built in CI | Installer plus a portable build with GStreamer bundled; not yet run on real hardware |
| iOS | Not started | Toolchain target only |

## Architecture

```
crates/
  xmpp-core/    XMPP protocol engine, no Dioxus dependency
                connection, IQ router, roster, chat, MUC, PubSub/PEP,
                OMEMO, Jingle control plane, Spaces, conferences
  storage/      SQLite + FTS5 system of record; Redb only as a legacy import
  media/        GStreamer webrtcbin media, Jingle/SDP, Galène and conference backends
  ui/           Dioxus presentation only
  platform/     Desktop/web/mobile abstractions
  host/         Optional public Host: HTTP pages over public PubSub content
  app/          Composition root, configuration, runtime
dev/
  reference-testbed/  Movim + Galener + Galène reference environment
```

`crates/host` serves public blog, post, community, Space and Story pages with
Atom/RSS feeds, sitemap, robots, share links, reporting, moderation and
role-based administration. `crates/vendor/` contains patched prose/xso crates;
it is not first-party code.

### Design decisions

1. Dioxus 0.7.10 with the current signals API.
2. `Arc<RwLock<T>>` for cross-thread state; Dioxus `Signal<T>` is not `Send`.
3. Direct `tokio-xmpp` integration from the prose-im/xmpp-rs git repository.
4. SQLite + FTS5 is the system of record; Redb is a one-release import path.
5. OMEMO 1:1 works; MUC OMEMO is experimental but interoperable.
6. Voice channels are MUC rooms backed by an XMPP SFU (Galener first, ProtoXEP
   second). The client does not speak the Galène WebSocket API.

See [docs/adr/](docs/adr/) for the decision records.

## Getting started

### Prerequisites

```bash
# Enter the dev shell (Rust 1.98.1, GTK/WebKit, GStreamer, xdotool)
nix develop
```

All builds and tests should run inside `nix develop`. The system toolchain does
not provide the native libraries Dioxus desktop and GStreamer need.

### Build and test

```bash
cargo check --workspace
cargo test -p xmpp-core -p storage --lib

cargo fmt --all
cargo clippy --workspace

# Desktop dev / production bundle
dx serve --platform desktop -p app
dx bundle --platform desktop -p app

# Web (currently fails at `xmpp-core`; see docs/STATUS.md)
dx serve --platform web -p app

# Rebuild the stylesheet after adding Tailwind classes
tailwindcss -c tailwind.config.js -i crates/ui/assets/tailwind.css \
  -o crates/ui/assets/app.css --minify
```

### Live tests

The default suite is hermetic. The Prosody and media suites are opt-in:

```bash
# Start the local Prosody fixture (alice/bob/admin)
./prosody/run.sh

JABU_TEST_XMPP=1 cargo test -p xmpp-core --test prosody_smoke -- --test-threads=1

# GStreamer / SFU integration (needs the Movim/Galener testbed)
JABU_TEST_XMPP=1 cargo test -p media --test conference_live -- --ignored --nocapture
```

The Movim/Galener/Galène testbed lives in
[dev/reference-testbed/README.md](dev/reference-testbed/README.md).

### Configuration

With no configured account the app shows a sign-in screen. An account can also
be configured in `config.toml`:

```toml
[xmpp]
jid = "user@example.com"
password = "optional"
resource = "jabu"
```

The optional public Host uses the same account and serves public blog pages:

```toml
[host]
bind = "127.0.0.1:8088"
public_url = "https://jabu.example"
site_name = "Jabu"
description = "Public pages served by a Jabu instance."
cache_ttl_secs = 300
db_path = "jabu-host.sqlite3"
admin_token = "change-me"
retention_days = 90
```

Run it with `jabu host` or `jabu-host --config host.toml`; put a TLS reverse
proxy in front. It never serves chats or private posts. `/report` collects
abuse reports and `/admin` holds the moderation queue, audit log, settings,
tokens and backup download. Point the app at it with `[reporting] host` to
enable in-app report actions, and open `jabu://` links from public pages with
`jabu --open <link>`. See [docs/HOST.md](docs/HOST.md) for deployment.

The SQLite database and saved credentials live in the platform data directory.
Credentials and the OMEMO encryption key use the OS keyring when one is
available; a successful write removes any older plaintext copy. OMEMO state is
encrypted with AES-256-GCM, and an encrypted state is never replaced with
plaintext when the keyring disappears. Without a keyring at all, the documented
SQLite fallback is used, so treat such a database as sensitive. See
[docs/STATUS.md](docs/STATUS.md) §6.

## Documentation

- [docs/STATUS.md](docs/STATUS.md) — what works today, XEP matrix, known gaps
- [docs/PLAN.md](docs/PLAN.md) — open milestones, risks, gates and next actions
- [docs/adr/README.md](docs/adr/README.md) — decision log index and architecture decisions
- [docs/HOST.md](docs/HOST.md) — public Host deployment, configuration and administration
- [docs/ANDROID.md](docs/ANDROID.md) — Android build and emulator setup
- [docs/WINDOWS.md](docs/WINDOWS.md) — Windows build, GStreamer and packaging
- [AGENTS.md](AGENTS.md) — repository guide for AI agents and contributors
- [docs/adr/](docs/adr/) — architecture decision records

## Differences from Movim (PHP reference)

- Client-side first: the app connects to an external XMPP server. The optional
  Host (`jabu host` / `jabu-host`) serves public pages and provides a
  moderation console.
- Desktop-first; Android and web are still target work.
- OMEMO supports 1:1 and MUC; MUC encryption remains experimental.
- Feed, communities and Stories cover a compatible subset, including comments,
  likes and in-app publishing.
- Public blog and post pages exist; community/space/story pages, report intake,
  moderation queues and instance administration do not.

## License

MIT License — see [LICENSE](LICENSE).

## Acknowledgments

- [Movim](https://movim.eu) — behaviour and protocol reference (clean-room)
- [prose-im/xmpp-rs](https://github.com/prose-im/xmpp-rs.git) — tokio-xmpp,
  minidom and jid
- [Dioxus](https://dioxuslabs.com) — Rust UI framework
- [XMPP Standards Foundation](https://xmpp.org) — protocol specifications
