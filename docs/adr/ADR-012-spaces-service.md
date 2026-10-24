# ADR-012: Spaces service discovery and voice channels

## Status

Accepted

## Context

XEP-0503 defines a *space* as a pubsub node whose `pubsub#type` is
`urn:xmpp:spaces:0`. A *spaces service* is any pubsub service that supports
node creation, configuration and subscriptions; it is **not** necessarily a
`spaces.<domain>` component. Earlier code hardcoded `spaces.<domain>`, which
fails on servers that only expose `pubsub.<domain>`.

Spaces also need voice channels. Movim's own deployment uses Galener, a
Jingle-to-Galène SFU, for larger calls; full-mesh Jingle does not scale to the
10+ participants we target.

## Decision

- Discover the spaces service from the server `disco#items` and each component's
  `disco#info`. A component advertising `urn:xmpp:spaces:0` is preferred; the
  first pubsub component advertising `pubsub#create-nodes` is the fallback.
- Resolve lazily, with `pubsub.<domain>` as the last automatic candidate, and
  allow an explicit override stored in `spaces.service`.
- Only register a node as a space after its `disco#info` confirms
  `pubsub#type = urn:xmpp:spaces:0`. Never treat every node of a shared pubsub
  component as a space.
- Public spaces use `pubsub#access_model=open`; private spaces use `authorize`.
- Model voice channels as MUC rooms marked in the bookmark extensions, not as
  separate protocols. Media for larger rooms goes through an SFU (Galène or
  Galener) rather than mesh; the channel is the signaling and presence layer.
- The local Prosody fixture grants `pubsub:create-node` to registered accounts
  through a small module, matching servers where users may create nodes.

## Consequences

- Spaces work on stock `pubsub.<domain>` deployments and on spaces-aware
  services without a configuration change.
- Servers that hide their pubsub component need a manual service override.
- Listing spaces on a generic pubsub service costs one `disco#info` per node;
  XEP-0462 type filtering is used when the server advertises it.
- Voice channels keep the existing Jingle control plane for 1:1 calls and use a
  Galène SFU for multi-party channels (see ADR-013), so the user-facing channel
  model does not change when the media backend does.
