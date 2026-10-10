# ADR-005: E2EE Scope

## Status

Accepted

## Context

End-to-end encryption (E2EE) via OMEMO is a feature in the reference Movim implementation but adds significant complexity to an MVP client.

## Decision

**No E2EE in MVP**. Omemo/e2ee removed entirely from the implementation.

## Rationale

- **MVP focus**: Core XMPP functionality (chat, roster, MUC) takes priority over encryption
- **Complexity**: OMEMO key management, device trust, and session handling add substantial code
- **Clean-room implementation**: Avoids dependency on external E2EE crates with uncertain maintenance
- **Future consideration**: E2EE can be added post-MVP as a separate effort

## Consequences

- `omemo` module removed from `xmpp-core` and `storage`
- No references to omemo in events, roster, chat, MUC, pubsub, processor, or storage
- `StorageBackend` trait has no omemo methods

## Alternatives Considered

- **OMEMO via omemo-rust**: Deferred to post-MVP
- **Double Ratchet via signal-protocol**: Deferred to post-MVP
- **No E2EE ever**: Not decided; E2EE may be added in future versions
