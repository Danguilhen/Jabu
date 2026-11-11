#!/usr/bin/env bash
# Stop the disposable testbed and restore the temporary firewall changes.
# The state directory is kept for inspection; clean.sh removes it.
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/env.sh"

stop_pid() {
    local name="$1"
    local file="$2"
    if [[ -f "$file" ]]; then
        local pid
        pid="$(cat "$file" 2>/dev/null || true)"
        if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
            for _ in $(seq 1 20); do
                kill -0 "$pid" 2>/dev/null || break
                sleep 0.25
            done
            kill -9 "$pid" 2>/dev/null || true
        fi
        rm -f "$file"
    fi
}

stop_pid "host" "$STATE/host.pid"
stop_pid "galene" "$STATE/galene.pid"
stop_pid "caddy" "$STATE/caddy.pid"
pkill -f "$STATE/galene" 2>/dev/null || true
pkill -f "$STATE/bin/caddy" 2>/dev/null || true

if [[ -f "$STATE/prosody/prosody.pid" ]]; then
    pid="$(cat "$STATE/prosody/prosody.pid" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
        sleep 1
        kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$STATE/prosody/prosody.pid"
fi

if command -v sudo >/dev/null 2>&1; then
    sudo /run/current-system/sw/bin/nixos-firewall-tool reset || true
fi

echo "Services stopped and temporary firewall rules reset."
echo "The test data remains in: $STATE"
