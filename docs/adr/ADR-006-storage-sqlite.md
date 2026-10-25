# ADR-006: SQLite + FTS5 Storage

## Status

Accepted. The SQLite/FTS5 decision is implemented. Credentials now use the OS
keyring on desktop with a SQLite fallback; the OMEMO store is **not** moved yet
and is still plaintext JSON in `settings`. See `docs/STATUS.md` §5–6.

## Context

The application needs durable local state for multiple accounts: contacts, conversations,
messages, MUC history, feeds, Spaces, Stories, calls, drafts, search, moderation and
administration. The existing redb backend stores opaque JSON values and has no full-text
search, relational queries or migration tooling.

## Decision

Use SQLite as the application's system of record, with FTS5 for full-text search. Use
`rusqlite` with bundled SQLite on desktop and Android. Keep Redb only as a one-release
import path for existing settings and OMEMO state.

## Rationale

- Relational queries for messages, rooms, posts, comments, reactions, reports and audit logs.
- FTS5 provides Movim-class search without maintaining a separate index.
- Transactions, migrations, WAL and backup are well understood.
- Bundled SQLite cross-compiles for Android; no server dependency.
- Account-scoped schema is straightforward.

## Consequences

- `StorageFactory` creates `SqliteStorage` by default.
- Schema migrations are versioned (`PRAGMA user_version` + `schema_migrations`) and tested from a legacy database.
- Credentials use the OS keyring/Keystore on desktop, with a SQLite fallback when no keyring service is available. OMEMO private state is still plaintext in `settings` and must move next.
- Web support will need a WASM SQLite or IndexedDB adapter later.

## Alternatives considered

- Redb: fast KV, but no FTS/relations and the current implementation is not wired.
- Separate search engine: unnecessary operational and binary-size cost.
