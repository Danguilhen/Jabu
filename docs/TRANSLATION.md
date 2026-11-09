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
| Hy-MT2-1.8B Q4_K_M | `Hy-MT2-1.8B-Q4_K_M.gguf` | 1.13 GB | Apache-2.0 | 8–16 GB desktops, capable phones |
| Hy-MT2-7B Q4_K_M | `Hy-MT2-7B-Q4_K_M.gguf` | 4.62 GB | Apache-2.0 | desktops with at least 16 GB RAM |

The pinned revisions and checksums are recorded in
`crates/translate/src/models.rs`. Nothing downloads automatically; the user
chooses a model in Settings → Translation.

### Hardware recommendations

| Device | Recommendation |
| --- | --- |
| Desktop, 16 GB RAM or more | 7B recommended; 1.8B also works |
| Desktop/laptop, 8–16 GB | 1.8B recommended |
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

Measured on a 31 GB, 20-core Linux host with CPU inference and 8 threads:

| Model | Warm load | Short Korean→French message |
| --- | ---: | ---: |
| Hy-MT2-1.8B Q4_K_M | ~0.25 s | ~6–11 s |
| Hy-MT2-7B Q4_K_M | ~0.46 s | ~23 s |

The 7B model is the quality choice, not the interactive one: CPU-only
translation is noticeably slower. A cold first load also pays for disk I/O and
page-in. GPU offload is not enabled yet. The 1.8B model is the better default
for laptops and for reading chat in real time.

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

## Limits

- Machine translation can be wrong. The original is always one click away.
- Very short messages are poor language-detection input; Jabu does not
  auto-translate when it is unsure.
- Each message is translated independently; conversational context is not sent
  to the model.
- Formatting is not translated. The translation view is plain text.
- The feature is desktop-first. Android support follows the mobile client.
