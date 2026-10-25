# ADR-009: Optional Host/Gateway for Public Pages and Administration

## Status

Accepted; not started. There is no `crates/host` yet.

## Context

Movim is a server-side web client. Its public blog/community/space/story pages, share links,
instance administration, moderation and user reporting are served by the instance. A pure
desktop/mobile app cannot serve public URLs.

## Decision

Add an optional headless `host` binary built from the same Rust crates. It provides
server-rendered public pages, OpenGraph/Atom/RSS output, share links, report intake,
moderation queues, instance administration and an audit log. The desktop app can run Host
mode; Android connects to a Host and provides the admin/moderation console.

## Rationale

- Preserves the user-facing parity requested without depending on a Movim PHP server.
- Public URLs, crawlers and share cards require an HTTP endpoint.
- Reuses protocol and storage code, keeping one implementation of content parsing.
- Host mode is optional; normal chat still works without it.

## Consequences

- A new `crates/host` crate and deployment/configuration surface.
- Public content cache retention, privacy and moderation policies become explicit.
- XMPP server-level administration is limited to XEP-0050/provider APIs; the Host does not replace an XMPP server.
- Desktop and Android admin UI share the same role-aware service API.

## Alternatives considered

- Omit public pages/admin: rejected by the project owner.
- Embed a full web server in every mobile app: rejected; unreliable on Android and unnecessary for clients.
