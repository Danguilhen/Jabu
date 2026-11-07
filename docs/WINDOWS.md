# Windows builds

The Windows desktop target is built in CI on `windows-latest`. It is the same
Dioxus desktop app as Linux, with calls and voice channels included.

## What CI produces

The `build-windows` job runs on every push to `main`/`develop` and on `v*` tags:

| Artifact | Contents |
|---|---|
| `windows-installer` | Installer from `dx bundle` (NSIS `.exe` and/or WiX `.msi`) |
| `windows-portable` | `jabu-<version>-windows-x64.zip` — `jabu.exe` plus the GStreamer runtime DLLs, plugins and plugin scanner |

The portable zip is the reliable way to get working calls: GStreamer is linked
at build time but must also be present at run time, and the installer does not
carry it.

## Prerequisites on the runner

1. **GStreamer 1.26.11 MSVC x86_64**, both packages, installed silently:
   - `gstreamer-1.0-msvc-x86_64-1.26.11.msi` (runtime)
   - `gstreamer-1.0-devel-msvc-x86_64-1.26.11.msi` (headers, import libraries,
     `.pc` files)

   `crates/media` compiles GStreamer for `linux`, `macos` and `windows`, so
   `gstreamer-sys` needs the development package at build time.

2. **pkg-config** — `gstreamer-sys` resolves its libraries through
   `pkg-config`, which the Windows image does not ship. CI installs
   `pkgconfiglite` with Chocolatey.

3. **`PKG_CONFIG_PATH`** pointing at
   `C:\gstreamer\1.0\msvc_x86_64\lib\pkgconfig`, and
   `C:\gstreamer\1.0\msvc_x86_64\bin` on `PATH`.

The GStreamer version matters: the workspace uses `gstreamer-rs` 0.25, which
tracks the 1.26 release line. Newer 1.28 installers are `.exe` files rather
than `.msi`, so the CI recipe pins 1.26.11.

## Building locally on Windows

```powershell
choco install pkgconfiglite -y
# install both GStreamer 1.26.11 MSVC x86_64 packages, then:
$env:PKG_CONFIG_PATH = "C:\gstreamer\1.0\msvc_x86_64\lib\pkgconfig"
$env:PATH = "C:\gstreamer\1.0\msvc_x86_64\bin;$env:PATH"

cargo install dioxus-cli --version 0.7.10 --locked
dx bundle --platform windows --release --package app
```

The bundle is written under `target/dx/jabu/bundle/windows/`.

## Known limitations

- Nothing has run on Windows hardware yet; the first green CI build only
  proves that it compiles and packages.
- The single-instance guard is a no-op on Windows (`#[cfg(not(unix))]`), so two
  windows can be opened.
- WebView2 must be present. Windows 10/11 ship it; the installer is generated
  by `dx` and does not bootstrap it.
- The installer alone gives an app with calls unavailable unless GStreamer is
  installed separately. Use the portable zip for a self-contained build.
- No code signing certificate, so SmartScreen warns on first launch.
