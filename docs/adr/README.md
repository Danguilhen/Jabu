# Architecture Decision Records

One record per decision. Edit a record in place when reality changes; write a
new record only for a genuinely new decision. A record is marked **Superseded**
only when its decision is obsolete rather than merely updated.

| ADR | Decision | Status |
|---|---|---|
| [001](ADR-001-ui-framework.md) | UI framework selection | Superseded (version by 007) |
| [002](ADR-002-xmpp-library.md) | XMPP library selection | Accepted |
| [003](ADR-003-concurrency-model.md) | Concurrency model | Accepted |
| [004](ADR-004-storage-backend.md) | Storage backend selection | Superseded by 006 |
| [005](ADR-005-e2ee-scope.md) | E2EE scope | Accepted (MUC OMEMO open) |
| [006](ADR-006-storage-sqlite.md) | SQLite + FTS5 storage | Accepted, implemented |
| [007](ADR-007-dioxus-version.md) | Dioxus version strategy | Accepted |
| [008](ADR-008-headless-core.md) | Headless core and app-service boundary | Accepted (boundary open) |
| [009](ADR-009-public-host.md) | Optional Host for public pages and admin | Accepted, implemented |
| [010](ADR-010-call-media.md) | Call media backend | Accepted (desktop); Android open |
| [011](ADR-011-emoji-assets.md) | Emoji assets and custom packs | Accepted |
| [012](ADR-012-spaces-service.md) | Spaces service discovery and voice channels | Accepted |
| [013](ADR-013-space-voice-channels.md) | Space voice channels over a Galène SFU | Superseded by 014 |
| [014](ADR-014-sfu-conferences.md) | Space voice channels as XMPP SFU conferences | Accepted |
| [015](ADR-015-omemo-at-rest.md) | OMEMO state encrypted at rest | Accepted (desktop) |
| [016](ADR-016-sasl2.md) | XEP-0388 SASL2 negotiation | Proposed |
| [017](ADR-017-parity-scope-and-release-order.md) | Parity scope and release order | Accepted |
| [018](ADR-018-target-protocol-coverage.md) | Target protocol coverage | Accepted |
| [019](ADR-019-internationalization.md) | Fluent catalogues and runtime translation | Accepted, implemented (14 locales; native review open) |
| [020](ADR-020-on-device-translation.md) | On-device message translation | Accepted; core, storage, desktop UI and GPU offload implemented; Android open |
