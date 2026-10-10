# Next Steps: Direct tokio-xmpp Implementation

## Approach
Replace prose-core-client dependency with direct tokio-xmpp usage. This avoids all upstream compatibility issues.

## Implementation Plan

### 1. Update Workspace Dependencies
Remove prose-core-client crates, add tokio-xmpp directly from xmpp-rs repo.

### 2. Implement Core XMPP Client
- Connection management (stream, TLS, SASL)
- Stanza routing (IQ, Message, Presence)
- Roster management
- Message handling (chat, MUC, receipts)
- MUC support
- OMEMO encryption (optional, can use omemo crate directly)

### 3. Storage Layer
Keep existing redb/rusqlite storage abstraction.

### 4. UI Integration
Keep existing Dioxus UI components, connect to new XMPP client.

## Files to Modify

1. `Cargo.toml` - Replace prose-core-client with tokio-xmpp
2. `crates/xmpp-core/` - Rewrite to use tokio-xmpp directly
3. `crates/app/src/main.rs` - Update to use new XMPP client
4. Keep storage, UI, platform crates as-is

## Dependencies to Add
```toml
tokio-xmpp = { git = "https://github.com/prose-im/xmpp-rs.git", branch = "main", package = "tokio-xmpp" }
xmpp-parsers = { git = "https://github.com/prose-im/xmpp-rs.git", branch = "main", package = "xmpp-parsers" }
minidom = "0.19"  # Use version compatible with tokio-xmpp
xso = { git = "https://github.com/prose-im/xmpp-rs.git", branch = "main", package = "xso" }
jid = { git = "https://github.com/prose-im/xmpp-rs.git", branch = "main", package = "jid" }
```

## Migration Steps
1. Remove prose-core-client vendor crates
2. Add tokio-xmpp dependencies
3. Rewrite xmpp-core to use tokio-xmpp
4. Update app to use new XMPP client
5. Test connection, auth, basic messaging

This approach gives us full control over the XMPP implementation and avoids all upstream compatibility issues.