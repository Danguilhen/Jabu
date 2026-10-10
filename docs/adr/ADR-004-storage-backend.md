# ADR-004: Storage Backend Selection

## Status

Accepted

## Context

The project requires a local embedded database for storing XMPP account data, contacts, chats, messages, and settings.

## Decision

Use **redb** as the embedded storage backend with an `Arc<dyn StorageBackend>` trait abstraction.

## Rationale

- **Embedded**: No separate database server required
- **Rust-native**: Pure Rust implementation, no C dependencies
- **ACID transactions**: Reliable data storage with transaction support
- **Trait abstraction**: `Arc<dyn StorageBackend>` allows swapping backends (SQLite, IndexedDB for WASM)
- **redb implementation**: Fast, embedded key-value store with serialization support

## Consequences

- `StorageBackend` trait defines the interface; `Arc<dyn StorageBackend>` for type erasure
- `redb` implementation in `crates/storage/src/redb_storage.rs`
- SQLite and IndexedDB backends exist as optional features but are not wired

## Alternatives Considered

- **SQLite**: Mature but C dependency, larger binary size
- **sled**: Embedded KV store but less active maintenance
- **IndexedDB**: WASM-only, not suitable for desktop
