# Android build and emulator

Status: the debug APK builds, installs, launches and logs in against the local
Prosody fixture on an x86_64 Android 14 (API 34) emulator. Notifications,
media and background delivery are not validated yet.

The Android target compiles without GStreamer: `media` is desktop-only and
`install_call_media` is compiled out on Android. Storage uses the activity's
`getFilesDir()`/`getCacheDir()` through JNI. The app ID is
`app.jabu`.

## Prerequisites

Run everything from the Nix dev shell:

```bash
nix develop
```

The shell provides Rust, Java, Gradle, `adb` and `dx`. The emulator, platform
and system image are not in the shell; install them into a writable SDK:

```bash
export ANDROID_HOME="$HOME/.android-sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
mkdir -p "$ANDROID_HOME/cmdline-tools"
curl -L -o /tmp/commandlinetools.zip \
  https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip
unzip -q /tmp/commandlinetools.zip -d "$ANDROID_HOME/cmdline-tools"
mv "$ANDROID_HOME/cmdline-tools/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"
yes | "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" \
  --sdk_root="$ANDROID_HOME" --licenses
"$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" \
  --sdk_root="$ANDROID_HOME" \
  "platform-tools" "emulator" "platforms;android-34" \
  "build-tools;34.0.0" "system-images;android-34;google_apis;x86_64"
```

### NixOS notes

The Google binaries are built for a generic glibc distribution and do not run
directly on NixOS.

- Use the nixpkgs NDK, not the generic r27c archive downloaded by the shell
  hook. Its root is the `ndk-bundle` directory:

  ```bash
  export ANDROID_NDK_HOME="$(NIXPKGS_ALLOW_UNFREE=1 nix eval --impure --raw \
    nixpkgs#androidenv.androidPkgs.ndk-bundle.outPath)/libexec/android-sdk/ndk-bundle"
  ```

- Rename the SDK's `adb` so `dx` falls back to the `adb` from the dev shell:

  ```bash
  mv "$ANDROID_HOME/platform-tools/adb" "$ANDROID_HOME/platform-tools/adb.sdk"
  ```

- Gradle's AAPT2 and the emulator are dynamically linked. Run them through
  `steam-run`:

  ```bash
  NIXPKGS_ALLOW_UNFREE=1 nix shell --impure nixpkgs#steam-run \
    -c steam-run "$(command -v dx)" build --platform android --package app
  ```

## Emulator

Create an AVD once:

```bash
export ANDROID_HOME="$HOME/.android-sdk"
echo no | "$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager" create avd \
  -n jabu-test -k "system-images;android-34;google_apis;x86_64" \
  -d pixel_5 --force
```

Start it (headless is enough for `adb`):

```bash
NIXPKGS_ALLOW_UNFREE=1 nix shell --impure nixpkgs#steam-run \
  -c steam-run "$HOME/.android-sdk/emulator/emulator" \
  -avd jabu-test -no-window -no-audio -no-boot-anim \
  -gpu swiftshader_indirect -no-snapshot -accel on
```

Wait for it:

```bash
adb start-server
adb devices
adb shell getprop sys.boot_completed   # 1
```

## Build, install, run

`dx` picks the emulator ABI (`x86_64`) from `adb shell uname -m`. Keep the
emulator running while building so it does not default to `aarch64`.

```bash
export ANDROID_HOME="$HOME/.android-sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_NDK_HOME=".../libexec/android-sdk/ndk-bundle"
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
export CC_x86_64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang"
export AR_x86_64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$CC_x86_64_linux_android"
export CARGO_TARGET_DIR="$HOME/.cache/jabu-android-target"
export TMPDIR="$HOME/.cache/jabu-tmp"
mkdir -p "$TMPDIR"

NIXPKGS_ALLOW_UNFREE=1 nix shell --impure nixpkgs#steam-run \
  -c steam-run "$(command -v dx)" build --platform android --package app
```

Install and launch:

```bash
APK="$CARGO_TARGET_DIR/dx/jabu/debug/android/app/app/build/outputs/apk/debug/app-debug.apk"
adb install -r "$APK"
adb shell am force-stop app.jabu
adb shell monkey -p app.jabu -c android.intent.category.LAUNCHER 1
```

Logs and a screenshot:

```bash
adb logcat -d | grep -E "RustStdoutStderr|FATAL|panicked"
adb shell screencap -p /sdcard/jabu.png
adb pull /sdcard/jabu.png /tmp/jabu-screen.png
```

## Local login test

The Prosody fixture in `prosody/` is enough for a login test:

```bash
./prosody/run.sh
adb reverse tcp:5222 tcp:5222
adb reverse tcp:5280 tcp:5280
```

Use a debug build, enable **Allow invalid TLS certificates (debug only)** and
sign in as `alice@localhost` with password `jabu-test`. The toggle is compiled
out of release builds. If the account was created with a different password,
delete `prosody/data/localhost/accounts/alice.dat` and run `prosody/run.sh`
again.

## Signed release and Obtainium

Obtainium installs a signed APK attached to a GitHub release; it does not
build from source. The release workflow expects four repository secrets:

- `ANDROID_KEYSTORE_BASE64` — base64 of the `.jks`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`

Create the keystore once and keep it safe. Every update must be signed with the
same key, otherwise Android refuses to replace the installed app:

```bash
keytool -genkeypair -v -keystore jabu-release.jks -alias jabu \
  -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 jabu-release.jks
```

Build a signed APK locally:

```bash
export ANDROID_KEYSTORE="$PWD/jabu-release.jks"
export ANDROID_KEYSTORE_PASSWORD='...'
export ANDROID_KEY_ALIAS=jabu
export ANDROID_KEY_PASSWORD='...'
export VERSION_NAME=0.1.0
export VERSION_CODE=1

NIXPKGS_ALLOW_UNFREE=1 nix shell --impure nixpkgs#steam-run \
  -c steam-run bash dev/android/build-release.sh
```

The script generates the Gradle project, patches `versionCode`,
`versionName` and the release signing config, runs `assembleRelease`, and
writes `dist/jabu-<version>.apk`. The update path was verified on the
emulator by installing `versionCode=1` and then `versionCode=2` with the same
key. Verify the APK with:

```bash
apksigner verify --print-certs dist/jabu-0.1.0.apk
```

On a `v*` tag, the `build-android-release` CI job builds the signed APK and the
`release` job attaches it to the GitHub release. `versionName` is the tag
without its leading `v`, and `versionCode` is the workflow run number, so each
tagged build installs as an update over the previous one. In Obtainium, add
`https://github.com/<owner>/<repo>` as a GitHub Releases source; it will pick
up `jabu-*.apk`.

The CI release APK is built for `aarch64`: `build-release.sh` passes
`--target aarch64-linux-android`. Without it, `dx` picks the host ABI when no
device is attached and produces an x86_64-only APK. Most phones are arm64; a
universal APK would need the other ABI libraries copied into the same APK.

## Notes and next steps

- `main` uses `Cli::parse_from(["jabu"])` on Android: the native
  activity does not provide a usable `argv`, and `std::env::args()` panics
  with a capacity overflow there.
- `tokio-xmpp` is vendored in `crates/vendor/tokio-xmpp` only to build
  `tokio-rustls` with the `ring` provider. `aws-lc-sys` does not link against
  Android's libc (`stdin`/`stderr` are undefined).
- Calls and GStreamer are disabled on Android. A media backend is still
  undecided (ADR-010/Gate B).
- XMPP login is verified against the local Prosody fixture (see above).
  Release builds use `webpki-roots`, so a real server must have a publicly
  trusted certificate or the client must be given a CA.
- The signed release path is scripted (`dev/android/build-release.sh`) and
  wired into the tag release workflow. The keystore exists and the four
  `ANDROID_*` repository secrets are stored; `v0.1.0` published the first
  signed APK. For the next release, bump the crate version and push a new `v*`
  tag. Keep a backup of the keystore outside the repository (`*.jks` is
  gitignored): every update must be signed with the same key.
