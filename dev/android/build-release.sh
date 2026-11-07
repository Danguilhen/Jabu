#!/usr/bin/env bash
# Build a signed Android release APK.
#
# Required:
#   ANDROID_KEYSTORE            absolute path to the .jks
#   ANDROID_KEYSTORE_PASSWORD
#   ANDROID_KEY_ALIAS
#   ANDROID_KEY_PASSWORD
#
# Optional:
#   VERSION_NAME   default: the app crate version
#   VERSION_CODE   default: 1 (must increase for every published release)
#   CARGO_TARGET_DIR
#   DX             default: dx
#   OUTPUT_APK     default: dist/jabu-<version>.apk
#
# On NixOS, run this script through steam-run so Gradle's AAPT2 can execute:
#   NIXPKGS_ALLOW_UNFREE=1 nix shell --impure nixpkgs#steam-run \
#     -c steam-run bash dev/android/build-release.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

: "${ANDROID_KEYSTORE:?set ANDROID_KEYSTORE to the release keystore}"
: "${ANDROID_KEYSTORE_PASSWORD:?set ANDROID_KEYSTORE_PASSWORD}"
: "${ANDROID_KEY_ALIAS:?set ANDROID_KEY_ALIAS}"
: "${ANDROID_KEY_PASSWORD:?set ANDROID_KEY_PASSWORD}"

if [[ ! -f "$ANDROID_KEYSTORE" ]]; then
  echo "keystore not found: $ANDROID_KEYSTORE" >&2
  exit 1
fi

# Gradle's file() resolves relative paths against the app module, not the
# repository root. Always hand it an absolute path.
ANDROID_KEYSTORE="$(cd "$(dirname "$ANDROID_KEYSTORE")" && pwd)/$(basename "$ANDROID_KEYSTORE")"
export ANDROID_KEYSTORE

VERSION_NAME="${VERSION_NAME:-$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "app"))')}"
VERSION_CODE="${VERSION_CODE:-1}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
DX="${DX:-dx}"

if ! [[ "$VERSION_CODE" =~ ^[0-9]+$ ]]; then
  echo "VERSION_CODE must be an integer, got: $VERSION_CODE" >&2
  exit 1
fi

echo "Building Rust release libraries for Android..."
"$DX" build --platform android --package app --release

GRADLE_FILE="$(find "$TARGET_DIR/dx" -path '*/release/android/app/app/build.gradle.kts' -print -quit)"
if [[ -z "$GRADLE_FILE" ]]; then
  echo "generated Gradle project not found under $TARGET_DIR/dx" >&2
  exit 1
fi
GRADLE_DIR="$(dirname "$(dirname "$GRADLE_FILE")")"

echo "Patching $GRADLE_FILE (versionCode=$VERSION_CODE, versionName=$VERSION_NAME)"
VERSION_NAME="$VERSION_NAME" VERSION_CODE="$VERSION_CODE" python3 - "$GRADLE_FILE" <<'PY'
import os
import re
import sys

path = sys.argv[1]
text = open(path).read()

text, n = re.subn(r"versionCode\s*=\s*\d+", f"versionCode = {os.environ['VERSION_CODE']}", text, count=1)
if n != 1:
    raise SystemExit("could not patch versionCode")
text, n = re.subn(r'versionName\s*=\s*"[^"]*"', f'versionName = "{os.environ["VERSION_NAME"]}"', text, count=1)
if n != 1:
    raise SystemExit("could not patch versionName")

if "signingConfigs {" not in text:
    signing = '''    signingConfigs {
        create("release") {
            storeFile = file(System.getenv("ANDROID_KEYSTORE"))
            storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
            keyAlias = System.getenv("ANDROID_KEY_ALIAS")
            keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
        }
    }
'''
    text = text.replace("    buildTypes {", signing + "    buildTypes {", 1)

needle = '        getByName("release") {\n'
if 'signingConfig = signingConfigs.getByName("release")' not in text:
    text = text.replace(
        needle,
        needle + '            signingConfig = signingConfigs.getByName("release")\n',
        1,
    )

open(path, "w").write(text)
PY

echo "Running Gradle assembleRelease..."
(
  cd "$GRADLE_DIR"
  ./gradlew --no-daemon assembleRelease
)

APK="$GRADLE_DIR/app/build/outputs/apk/release/app-release.apk"
if [[ ! -f "$APK" ]]; then
  echo "release APK not found at $APK" >&2
  exit 1
fi

OUT="${OUTPUT_APK:-$ROOT/dist/jabu-${VERSION_NAME#v}.apk}"
mkdir -p "$(dirname "$OUT")"
cp "$APK" "$OUT"

echo "Signed APK: $OUT"
echo "  versionName: $VERSION_NAME"
echo "  versionCode: $VERSION_CODE"
