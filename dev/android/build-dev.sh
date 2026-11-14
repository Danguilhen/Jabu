#!/usr/bin/env bash
# Build a debug APK with a separate application id.
#
# The dev APK can be installed beside a daily Jabu release, so testing never
# replaces app.jabu or its data. It uses the debug signing key and is not for
# release.
#
# Optional:
#   ANDROID_APPLICATION_ID  default: app.jabu.dev
#   ANDROID_TARGET          default: aarch64-linux-android
#   CARGO_TARGET_DIR        default: $ROOT/target
#   OUTPUT_APK              default: dist/jabu-dev.apk
#
# On NixOS, run through steam-run:
#   NIXPKGS_ALLOW_UNFREE=1 nix shell --impure nixpkgs#steam-run \
#     -c steam-run bash dev/android/build-dev.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

ANDROID_APPLICATION_ID="${ANDROID_APPLICATION_ID:-app.jabu.dev}"
ANDROID_TARGET="${ANDROID_TARGET:-aarch64-linux-android}"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
OUTPUT_APK="${OUTPUT_APK:-$ROOT/dist/jabu-dev.apk}"
DX="${DX:-dx}"
export CARGO_TARGET_DIR
export TMPDIR="${TMPDIR:-$ROOT/target/jabu-tmp}"
mkdir -p "$TMPDIR"

if [[ "$ANDROID_APPLICATION_ID" == "app.jabu" ]]; then
    echo "Refusing to build a dev APK with the release application id." >&2
    exit 1
fi

echo "Building dev Android libraries ($ANDROID_TARGET)..."
"$DX" build --platform android --package app --target "$ANDROID_TARGET"

GRADLE_FILE="$(find "$CARGO_TARGET_DIR/dx" -path '*/debug/android/app/app/build.gradle.kts' -print -quit)"
if [[ -z "$GRADLE_FILE" ]]; then
    echo "generated Gradle project not found under $CARGO_TARGET_DIR/dx" >&2
    exit 1
fi
GRADLE_DIR="$(dirname "$(dirname "$GRADLE_FILE")")"

echo "Setting applicationId=$ANDROID_APPLICATION_ID in $GRADLE_FILE"
ANDROID_APPLICATION_ID="$ANDROID_APPLICATION_ID" python3 - "$GRADLE_FILE" <<'PY'
import os
import re
import sys

path = sys.argv[1]
text = open(path).read()
application_id = os.environ["ANDROID_APPLICATION_ID"]
text, count = re.subn(
    r'applicationId\s*=\s*"[^"]*"',
    f'applicationId = "{application_id}"',
    text,
    count=1,
)
if count != 1:
    raise SystemExit("could not patch applicationId")
open(path, "w").write(text)
PY

echo "Running Gradle assembleDebug..."
(
    cd "$GRADLE_DIR"
    ./gradlew --no-daemon assembleDebug
)

APK="$GRADLE_DIR/app/build/outputs/apk/debug/app-debug.apk"
if [[ ! -f "$APK" ]]; then
    echo "debug APK not found at $APK" >&2
    exit 1
fi
mkdir -p "$(dirname "$OUTPUT_APK")"
cp "$APK" "$OUTPUT_APK"
echo "Dev APK: $OUTPUT_APK"
echo "  applicationId: $ANDROID_APPLICATION_ID"
echo "This APK is separate from app.jabu; install it with: adb install -r '$OUTPUT_APK'"
