# ADR-017: Parity Scope and Release Order

## Status

Accepted.

## Context

Jabu aims to match Movim's user-facing behaviour without reusing its AGPL
source. Movim is a server-side web client: public pages, share links, instance
administration and moderation are part of the product, not add-ons. The
protocol engine, UI and platform backends must therefore support desktop,
Android and the web, but they cannot all reach the same maturity at once.

## Decision

1. The goal is **full user-facing parity**: chat, rooms, encryption, calls,
   feeds, communities, Spaces, Stories, public pages, share links, reporting,
   moderation and instance administration.
2. The release order is **Linux desktop, then Android, then the web**.
   Desktop gates the first releases. Android follows once a native media
   backend and platform integration exist. The web is a target but stays
   blocked until a wasm-safe protocol/crypto path and transport exist.
3. Movim is a **behaviour and specification reference only**. The
   implementation is clean-room; do not copy its AGPL source, catalogues or
   assets.

## Rationale

- Matches the project owner's requested parity surface.
- Desktop has the fewest platform unknowns and is already the usable target.
- Publishing a platform before its media, storage and notification paths work
  would create a false release claim.

## Consequences

- Every platform must eventually cover navigation, notifications, calls,
  encryption, deep links and account management.
- A feature that only works on desktop stays marked partial in
  `docs/STATUS.md`; it does not close a milestone.
- Changing the scope or release order means editing this ADR, not writing a
  successor.

## Alternatives considered

- **Desktop only**: rejected; parity includes public pages and mobile.
- **Web first**: rejected; `xmpp-core` cannot build for wasm yet and a
  server-side parity product needs the Host first.
