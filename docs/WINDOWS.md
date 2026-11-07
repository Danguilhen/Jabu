# Windows builds

The Windows desktop target is built in CI on `windows-latest`. It is the same
Dioxus desktop app as Linux, with calls and voice channels included.

## What CI produces

The `build-windows` job runs on every push to `main`/`develop` and on `v*` tags:

| Artifact | Contents |
|---|---|
| `windows-installer` | `Jabu_<version>_x64-setup.exe` from `dx bundle` (NSIS) |
| `windows-portable` | `jabu-<version>-windows-x64.zip` — `jabu.exe` plus the GStreamer runtime DLLs, plugins and plugin scanner |

The bundle lands in `target/dx/jabu/bundle/windows/`: `jabu.exe` next to it and
the installer under `nsis/`. The installer is roughly 220 MB; the portable zip
is roughly 120 MB.

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

2. **pkgconf** — `gstreamer-sys` resolves its libraries through `pkg-config`,
   which the Windows image does not ship. CI installs the `pkgconf` MSI and
   points `PKG_CONFIG` at it. Do not use `pkgconfiglite`: it splits
   `PKG_CONFIG_PATH` on `:`, so `C:\...` becomes `C` plus `\...`.

3. **`PKG_CONFIG_PATH`** pointing at the `lib\pkgconfig` directory of the
   installed tree, and its `bin` on `PATH`. Silent `msiexec` installs under
   `C:\Program Files\gstreamer`, not `C:\gstreamer\1.0\msvc_x86_64`, so CI
   locates the tree by searching for `glib-2.0.pc` instead of assuming a path.

The GStreamer version matters: the workspace uses `gstreamer-rs` 0.25, which
tracks the 1.26 release line. Newer 1.28 installers are `.exe` files rather
than `.msi`, so the CI recipe pins 1.26.11.

## Building locally on Windows

```powershell
# install the pkgconf MSI, then both GStreamer 1.26.11 MSVC x86_64 packages
$root = "C:\Program Files\gstreamer"   # check where the installer put it
$env:PKG_CONFIG = "C:\Program Files\pkgconf-3.0.10\pkgconf.exe"
$env:PKG_CONFIG_PATH = "$root\lib\pkgconfig"
$env:PATH = "$root\bin;$env:PATH"

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
