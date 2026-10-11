#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export JABU_PROSODY_DIR="$ROOT/prosody"
CONFIG="$JABU_PROSODY_DIR/config/prosody.cfg.lua"

PIDFILE="$JABU_PROSODY_DIR/data/prosody.pid"

if [ -f "$PIDFILE" ]; then
    PID="$(cat "$PIDFILE" 2>/dev/null || true)"
    if [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; then
        kill "$PID"
        for _ in $(seq 1 50); do
            kill -0 "$PID" 2>/dev/null || exit 0
            sleep 0.2
        done
        kill -9 "$PID" 2>/dev/null || true
    fi
    rm -f "$PIDFILE"
else
    prosodyctl --config "$CONFIG" --force stop 2>/dev/null || true
fi
