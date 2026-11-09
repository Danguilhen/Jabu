# ADR-021: Translation runtime isolation

- Status: Accepted; GGUF preflight, the helper process, the Linux bwrap sandbox,
  idle shutdown and an optional memory limit are implemented; macOS/Windows
  sandboxing and mobile isolation remain open
- Date: 2026-11-08
- Owners: Jabu client
- Related: [ADR-020](ADR-020-on-device-translation.md)

## Context

ADR-020 makes translation local, but the translation runtime is native C++
inside `llama.cpp`. A GGUF file is untrusted input to that parser. A malformed
or malicious model could exploit a memory-safety bug, and an in-process crash
can take down the whole chat client. Downloaded models are pinned and
checksummed; user-supplied models have no such guarantee.

Translation also needs a hard resource boundary. A model can consume several
gigabytes and many CPU threads; the chat client must remain responsive.

## Decision

### Always preflight the container

Before llama.cpp loads a file, a pure-Rust GGUF inspector checks:

- magic and version,
- metadata and tensor counts,
- string and array lengths,
- metadata byte budget,
- tensor dimensions and offsets,
- the `general.architecture` value.

Jabu only accepts `hunyuan-dense`. The preflight is not a full tensor-layout
validator; llama.cpp remains the authority for tensor data. It removes the
cheap malformed-file and resource-exhaustion cases before native parsing.

### Offer a desktop helper process

The desktop can run `jabu-translate-helper` as a separate process:

- model path, model id, thread count and GPU layers are command-line arguments,
- requests and responses are JSON lines over inherited stdin/stdout,
- the helper opens no network listener,
- the parent kills and restarts the helper on failure,
- `kill_on_drop` guarantees cleanup if the app exits.

`translation.backend = "sidecar"` selects it. The default remains `embedded`.

On Linux, `translation.sandbox = true` runs the helper under bubblewrap with a
private network namespace, a read-only model, a temporary `/tmp`, and only the
system/driver paths the helper needs. The helper stops after the idle timeout
and can apply an address-space limit (`max_memory_mb`). GPU access requires `/dev/dri` and the
Vulkan driver paths; CPU-only is the tighter profile. If bwrap cannot start,
the helper fails closed instead of silently running unsandboxed.

macOS should use a sandbox profile; Windows should use a Job Object and, later,
AppContainer. Those are still open.

Android cannot use a desktop sidecar. The Android app process is the isolation
boundary; the in-process backend runs with the mobile app sandbox and stricter
memory/thermal controls.

## Consequences

Positive:

- malformed GGUF files are rejected before native parsing,
- a helper crash does not kill the chat client,
- the helper can be killed to reclaim memory,
- the path to an OS sandbox and resource limits is explicit.

Negative:

- a second binary must be packaged for desktop releases,
- stdin/stdout adds a small IPC hop,
- the helper duplicates some model lifecycle logic,
- OS sandboxing around a GPU is less strict than CPU-only sandboxing,
- macOS and Windows still lack an OS sandbox.

## Alternatives considered

- **In-process only.** Rejected as the final state: no crash isolation and no
  resource boundary.
- **OS sandbox around the whole app.** Rejected: the app needs network, file
  dialogs and media devices.
- **External local server only.** Useful as an advanced fallback, but it is
  not bundled, not automatic, and depends on the user’s service.
- **Pure-Rust inference.** Interesting for Android and long-term memory safety,
  but not mature or fast enough to replace llama.cpp today.

## References

- [ADR-020](ADR-020-on-device-translation.md)
- [TRANSLATION.md](../TRANSLATION.md)
