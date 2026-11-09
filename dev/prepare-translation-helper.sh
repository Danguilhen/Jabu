#!/usr/bin/env bash
# Stage the translation sidecar under the name tauri/dioxus expects:
# <base>-<target-triple>[.exe]. Run this before `dx bundle`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

case "$(uname -s)" in
    Linux) FEATURES=(embedded vulkan) ;;
    Darwin) FEATURES=(embedded metal) ;;
    *) FEATURES=(embedded) ;;
esac

TRIPLE="$(rustc -vV | awk '/^host:/ { print $2 }')"
if [[ "$TRIPLE" == *windows* ]]; then
    EXE=".exe"
else
    EXE=""
fi

cargo build --release -p jabu-translate --features "${FEATURES[*]}" \
    --bin jabu-translate-helper

mkdir -p dist/bin
cp "target/${TRIPLE}/release/jabu-translate-helper${EXE}" \
    "dist/bin/jabu-translate-helper-${TRIPLE}${EXE}"
chmod +x "dist/bin/jabu-translate-helper-${TRIPLE}${EXE}"
echo "Staged dist/bin/jabu-translate-helper-${TRIPLE}${EXE}"
