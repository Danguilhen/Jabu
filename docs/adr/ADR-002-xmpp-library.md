# ADR-002: XMPP Library Selection

## Status

Accepted

## Context

The project requires an XMPP client library for Rust that supports connection management, roster, chat, MUC, PubSub, and event processing.

## Decision

Use **tokio-xmpp** directly from the [prose-im/xmpp-rs](https://github.com/prose-im/xmpp-rs.git) git repository (branch `main`).

## Rationale

- **Direct tokio-xmpp**: More control over XMPP protocol handling than higher-level wrappers
- **xmpp-rs git repo**: Latest features and bug fixes not yet crates.io published
- **Direct dependency management**: `patch.crates-io` forces xmpp-rs git versions for all related crates (xmpp-parsers, minidom, jid, xso, xso_proc, tokio-xmpp)
- **Avoids prose-xmpp abstraction**: prose-xmpp vendored crate was considered but adds unnecessary abstraction layer

## Consequences

- XMPP dependencies use git references instead of crates.io versions
- `patch.crates-io` in workspace `Cargo.toml` pins all xmpp-rs crates to git main branch
- `prose-xmpp` vendored crate exists but is not used directly

## Alternatives Considered

- **prose-xmpp**: Higher-level wrapper but adds abstraction; vendored crate kept but unused
- **tokio-xmpp from crates.io**: Older version, lacks latest features
