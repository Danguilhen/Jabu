# ADR-020: On-device message translation

- Status: Accepted; core, storage and desktop UI implemented; Android and
  hardening open
- Date: 2026-11-08
- Owners: Jabu client
- Supersedes: none

## Context

Jabu can show conversations in fifteen UI languages, but it cannot help two
people who do not share a language. A Korean speaker and a French speaker can
already exchange XMPP messages; neither can read the other's text.

Cloud translation is the obvious shortcut and the wrong one for this project.
Messages may be OMEMO-encrypted, and the only party that should ever see the
plaintext is the receiving device. Sending message text to a translation API
would turn an end-to-end encrypted conversation into a plaintext upload. It
would also add an availability dependency, a per-message cost and a new data
retention question.

The Hy-MT2 family makes a local implementation practical:

- 1.8B and 7B dense models, plus a 30B-A3B MoE that llama.cpp does not support
  for the dense chat path we need.
- 33 languages, which covers every language Jabu ships and more.
- Apache-2.0 GGUF weights; MIT llama.cpp.
- 1.8B Q4_K_M is 1.13 GB; 7B Q4_K_M is 4.62 GB.
- Desktop CPUs can run the 1.8B model; 16 GB desktops can run the 7B model.

Not every device can hold a model, and no device should be forced to. The
feature therefore has to be optional, opt-in, capability-aware and useful when
it is absent.

## Decision

### Translation is local, after decryption

Translation runs on the receiving device, after OMEMO or any other transport
decryption. Message plaintext and translated text are never sent over XMPP,
never posted to a Host, and never included in telemetry. No translation
metadata is added to stanzas, carbons, MAM or receipts. The server must not be
able to tell that a message was translated.

The engine is linked into Jabu as a native library. The model weights are a
separate, optional download. The base installer does not contain weights.

### The original message is immutable

A translation is an annotation. `ChatMessage::body` and `MucMessage::body` are
never replaced. Translations live in a separate local table keyed by account,
conversation, message, target language and model. The UI shows the translation
as the primary reading surface when it exists, with a one-click original. A
failed or absent translation leaves the original untouched.

### One model family, two sizes

The shipped manifest contains two pinned GGUF files:

| Model | Quantization | Size | Recommendation |
| --- | --- | ---: | --- |
| Hy-MT2-1.8B | Q4_K_M | 1.13 GB | 8–16 GB desktops, capable phones |
| Hy-MT2-7B | Q4_K_M | 4.62 GB | desktops with at least 16 GB RAM |

URLs are pinned to a Hugging Face revision and a SHA-256. The 1.25-bit STQ
variant is deliberately not shipped yet: its kernel is still an open llama.cpp
pull request and is ARM-only. Q4_K_M is the portable baseline.

Model selection is a recommendation, not a gate. The user can install the
smaller model on a large machine, try the larger model on a machine below the
recommendation, or keep no model at all.

### Embedded runtime, optional local endpoint

The default backend is llama.cpp through a Rust binding, compiled into Jabu
behind the `translation-llama` feature. A pure-Rust Candle backend is a
candidate for later evaluation, especially for Android, but it is not the
default while the project is young.

An advanced setting can point at an OpenAI-compatible endpoint (Ollama,
`llama-server`, LM Studio). Loopback is required by default, a bearer token is
stored in the keyring, redirects are disabled and the consent dialog states
that message text leaves the Jabu process. Remote endpoints require an explicit
override. This is a fallback for users who already run a local model server or
whose hardware cannot run the embedded engine; it is not the default and not a
Jabu service.

### Hardware and lifecycle policy

- Detect total and available RAM, physical cores, architecture and CPU
  features; expose the result in settings.
- Recommend 7B at 16 GB RAM and above, 1.8B from 8 GB, and no model below that.
- Check free RAM before loading. Refuse with an actionable message rather than
  letting the OOM killer decide.
- Set the context window explicitly (4096 tokens). Hy-MT2 advertises a 262k
  context; accepting the default can allocate tens of GB of KV cache.
- Use greedy decoding with a repeat penalty for consistent translations.
- Unload after an idle period, and aggressively on mobile.
- On mobile, auto-translation is disabled while backgrounded and respects
  low-RAM and thermal signals.

### Scope of the first release

- Desktop first; the Android client is not usable yet.
- Incoming direct messages and room messages.
- Manual translation by default, with global and per-chat automatic toggles.
- No automatic translation of outgoing messages: changing what the peer
  receives is a separate consent decision.
- No server-side or Host-side translation.

## Security invariants

1. No message body, prompt or translation is logged at any level.
2. No message body or translation is present in crash reports or telemetry.
3. Translation never writes to XMPP stanzas or the Host database.
4. Model files are checksum-verified and installed atomically with private
   permissions.
5. User-supplied model files are untrusted and are never loaded automatically.
6. URLs, mentions, code and custom emoji are protected with placeholders and
   restored from the original, so model output cannot introduce links.
7. Model output is rendered as plain text; it cannot create actions.
8. Translations are as sensitive as messages at rest. They are stored in the
   app-private database, deleted with their message/conversation/account, and
   can be disabled.
9. External endpoints are loopback-only unless the user explicitly accepts the
   off-device warning.
10. The translation runtime is pinned and updated deliberately; a model update
    is never silent.

## Consequences

Positive:

- Cross-language conversations work offline and without a service.
- The privacy property is easy to state and audit: content never leaves the
  device.
- The feature degrades gracefully: no model means no translation, not a broken
  chat.
- A single multilingual model avoids leaking the language pair through model
  selection.

Negative:

- A native C++ dependency enters the build, with platform-specific packaging.
- Model download, storage and memory management become product surfaces.
- Translation quality is machine quality; the original must stay visible and
  users must be told.
- The 7B model is slow on CPU-only machines; GPU offload is optional and not a
  release requirement.
- Android support is deferred until the client itself is usable.

## Alternatives considered

- **Cloud translation API.** Rejected: breaks the E2EE privacy model, adds a
  service dependency and retains message text.
- **Jabu-hosted translation service.** Rejected for the same reason; a Host
  cannot read OMEMO plaintext anyway.
- **Bundle weights in the installer.** Rejected: 1.1–4.6 GB per platform,
  difficult updates, and most users never enable the feature.
- **Bundled sidecar process.** Deferred. It gives crash isolation and a smaller
  Rust attack surface, but complicates packaging and cannot be used on mobile.
  The engine trait leaves room for it.
- **Per-language-pair models.** Rejected: more downloads, worse coverage and
  metadata leakage about which pairs a user needs.
- **Pure-Rust Candle engine.** Not selected as the default yet. It avoids the
  C++ build and cross-compiles well, but the Hy-MT implementation is young and
  slower; revisit after the desktop spike.

## References

- [Hy-MT2 model card](https://huggingface.co/tencent/Hy-MT2-1.8B-GGUF)
- [Hy-MT2 technical report](https://arxiv.org/abs/2605.22064)
- [llama.cpp](https://github.com/ggml-org/llama.cpp)
- [ADR-005](ADR-005-e2ee-scope.md) and [ADR-015](ADR-015-omemo-at-rest.md)
- [ADR-019](ADR-019-internationalization.md)
