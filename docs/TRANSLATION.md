# On-device translation

Jabu can translate incoming messages on the device. The original message is
always kept; the translation is a separate annotation.

The decision and its security invariants live in
[ADR-020](adr/ADR-020-on-device-translation.md). This page is the practical
guide.

## What leaves the device

In the default embedded mode, nothing. Message text, prompts and translations
are processed locally and stored in the local database. They are never sent
over XMPP, never posted to a Host and never included in telemetry.

In that mode the only network request in the feature is the model download. It fetches a
pinned GGUF file from Hugging Face and verifies its SHA-256. You can skip the
download and import a model file yourself if you prefer.

An advanced setting can connect to a local OpenAI-compatible server. That sends
message text to that server, so it is off by default and requires an explicit
consent. A remote server requires an additional override.

## Models

| Model | File | Size | License | Typical use |
| --- | --- | ---: | --- | --- |
| Hy-MT2-1.8B Q4_K_M | `Hy-MT2-1.8B-Q4_K_M.gguf` | 1.13 GB | Apache-2.0 | **default** on desktops and capable phones |
| Hy-MT2-7B Q4_K_M | `Hy-MT2-7B-Q4_K_M.gguf` | 4.62 GB | Apache-2.0 | opt-in quality choice on large desktops |

The pinned revisions and checksums are recorded in
`crates/translate/src/models.rs`. Nothing downloads automatically; the user
chooses a model in Settings → Translation.

### Hardware recommendations

| Device | Recommendation |
| --- | --- |
| Desktop/laptop, 8 GB RAM or more | 1.8B default; 7B is opt-in |
| Desktop, 16 GB RAM or more | 1.8B default; 7B available for quality |
| Desktop, less than 8 GB | No model by default; use the external endpoint |
| Phone, 8 GB RAM or more | 1.8B, on demand |
| Phone, low RAM or thermal-limited | Off by default |

These are recommendations. A user can try a model anyway; Jabu warns when free
RAM is below the safety margin instead of letting the system OOM-kill the app.

The 1.25-bit STQ model is not offered yet: its llama.cpp kernel is an open pull
request and only supports ARM today. Q4_K_M is the portable choice.

## Using it

1. Open Settings → Translation.
2. Install the recommended model in Settings, or point `translation.model` at a local `.gguf` file in `config.toml`.
3. Leave the mode on **Manual**, enable **Translate incoming messages** for
   every chat, or use the languages button in a chat header to enable it for
   that conversation only.
4. Open a message's action menu and choose **Translate**.

When a translation exists, the translated text is shown first with a
"Translated from Korean · Machine translation" label. **Show original** reveals
the stored message. A failed translation shows the original and a retry action.

Only incoming messages are translated automatically. Outgoing messages are
never rewritten.

## Performance

Measured on a 31 GB, 20-core Linux host:

| Model | CPU warm load | CPU short message | Vulkan short message |
| --- | ---: | ---: | ---: |
| Hy-MT2-1.8B Q4_K_M | ~0.25 s | ~6–11 s | ~3.5 s |
| Hy-MT2-7B Q4_K_M | ~0.46 s | ~23 s | ~5.9 s (16/33 layers) |

The 7B model is the quality choice, not the interactive one on CPU. The 1.8B
model is the default for laptops and for reading chat in real time.

GPU offload is automatic when Jabu is built with a GPU backend and a device is
available:

- Linux builds enable Vulkan; macOS uses Metal. Windows currently ships the CPU
  backend; the `vulkan` feature can be enabled when a Vulkan toolchain is present.
- `gpu_layers = 0` (default) lets Jabu choose. It offloads all layers when the
  model fits comfortably in GPU memory, uses a partial offload for a larger
  model, and falls back to CPU when no GPU is available.
- `gpu_layers = -1` forces CPU; a positive number sets the layer count.

On the same host with an RTX 4050 Laptop GPU (6 GB), the 1.8B model used all
layers and took ~3.5 s for the test message instead of ~6–11 s on CPU. The 7B
model offloaded 16/33 layers and took ~5.9 s instead of ~23 s on CPU.

## Helper process

For crash isolation, build the helper next to the app:

```bash
cargo build -p jabu-translate --features vulkan --bin jabu-translate-helper
```

For a bundle, stage it under the name the Dioxus/Tauri bundler expects before
`dx bundle`:

```bash
./dev/prepare-translation-helper.sh
dx bundle --platform desktop --release --package app
```

The script picks Vulkan on Linux, Metal on macOS, and copies the helper to
`dist/bin/jabu-translate-helper-<target-triple>`.

On Windows, CI stages the helper under both the bare target-triple name and the
`.exe` name: Dioxus 0.7.10 appends the target triple but not `.exe` when it
resolves `external_bin`. The NSIS hook renames the extensionless copy, so
`SidecarConfig::default_helper_path()` finds `jabu-translate-helper.exe` after
installation.

Then select it:

```toml
[translation]
backend = "sidecar"
```

The helper receives the model path, model id, thread count and GPU layers as
arguments. Requests and responses are JSON lines over stdin/stdout; it opens
no network listener. `kill_on_drop` cleans it up if the app exits.

On Linux, `sandbox = true` runs the helper under bubblewrap with a private
network namespace, a read-only model file and a temporary `/tmp`:

```toml
[translation]
backend = "sidecar"
sandbox = true
```

`bwrap` must be installed. If it cannot start, translation fails closed rather
than silently running unsandboxed. GPU access still works through `/dev/dri`
and the Vulkan driver paths; CPU-only is the tighter profile. macOS and
Windows sandboxing is still open. Release packaging must copy the helper next
to the app binary.

The helper is stopped after `idle_unload_secs` of inactivity, and the optional
`max_memory_mb` sets an address-space limit inside the helper. A limit that is
too small makes the helper fail to load the model; zero leaves it unlimited.

## Notifications

Notifications show the original message immediately. If translation is enabled
for that conversation, Jabu translates in the background and replaces the
notification with the translated body once it is ready. The replacement uses
the same notification id and is silent.

This is off by default because a translated message on a lock screen is still
sensitive:

```toml
[translation]
translate_notifications = true
```

The original message is always what is stored; the notification is only a
view.

## Privacy details

- OMEMO is decrypted first; translation only sees the already-decrypted text.
- Translations are stored locally and are as sensitive as the messages.
- Deleting a message, conversation or account removes its translations.
- No body, prompt or translation is written to logs.
- Model files are installed with private permissions and verified before use.
- User-supplied model files are treated as untrusted.
- URLs, mentions, code and custom emoji are protected during translation and
  restored from the original text.

## External endpoint

Advanced users can point Jabu at an OpenAI-compatible server:

```toml
[translation]
endpoint = "http://127.0.0.1:11434/v1"
model = "hy-mt2:1.8b"
# token = "..."        # prefer the keyring
allow_remote = false   # never set this for a service you do not control
```

Loopback is required unless `allow_remote` is set. Redirects are disabled so a
token cannot be forwarded to another host. The endpoint is configured in
`config.toml`; a token field is supported but keyring storage is still on the
hardening list.

## Development

The headless crate is `crates/translate`. It has no Dioxus dependency.

```bash
cargo test -p jabu-translate --lib
```

Model-backed tests are ignored unless a model path is provided:

```bash
JABU_TEST_TRANSLATE_MODEL=/path/to/Hy-MT2-1.8B-Q4_K_M.gguf \
  cargo test -p jabu-translate --test model -- --ignored --nocapture
```

The embedded llama.cpp backend is behind the `embedded` feature. Building it
needs CMake and clang/libclang; `nix develop` provides both and exports
`LIBCLANG_PATH`. Android builds do not enable the feature, so they keep the
unavailable backend until the mobile client is ready.

## Remaining work

- Verify `jabu-translate-helper` is present in the AppImage, macOS app, NSIS
  installer and Windows portable ZIP. The staging script and `external_bin`
  entry exist, but local `dx bundle` could not complete because its linuxdeploy
  lookup failed.
- Live-test notification replacement with a real background XMPP message.
- Add macOS/Windows sandboxing and a CPU rlimit.
- Add global translation search and batch conversation translation.
- Add the external-endpoint UI, keyring token storage and remote consent.
- Bring the embedded backend to Android with memory/thermal policy.
- Native review of the UI strings and model quality samples.
- Decide whether translations need database encryption at rest.

## Limits

- Machine translation can be wrong. The original is always one click away.
- Very short messages are poor language-detection input; Jabu does not
  auto-translate when it is unsure.
- Each message is translated independently; conversational context is not sent
  to the model.
- Formatting is not translated. The translation view is plain text.
- The feature is desktop-first. Android support follows the mobile client.
