#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JABU_DIR="$ROOT/movim"
PGDATA="$ROOT/postgres"
LOGS="$ROOT/logs"

if [ -S "$JABU_DIR/cache/socketapi.sock" ]; then
    (cd "$JABU_DIR" && php daemon.php galener stop) >/dev/null 2>&1 || true
fi

if [ -f "$LOGS/nginx.pid" ]; then
    nginx -s stop -c "$LOGS/nginx.conf" >/dev/null 2>&1 || true
fi
if [ -f "$LOGS/php-fpm.pid" ]; then
    kill "$(cat "$LOGS/php-fpm.pid")" 2>/dev/null || true
    rm -f "$LOGS/php-fpm.pid"
fi

for name in web daemon; do
    if [ -f "$LOGS/$name.pid" ]; then
        pid="$(cat "$LOGS/$name.pid")"
        kill "$pid" 2>/dev/null || true
        rm -f "$LOGS/$name.pid"
    fi
done

if pg_ctl -D "$PGDATA" status >/dev/null 2>&1; then
    pg_ctl -D "$PGDATA" stop -m fast >/dev/null 2>&1 || true
fi

pkill -f '[m]ovim-testbed/movim/daemon.php start' 2>/dev/null || true
pkill -f '[m]ovim-testbed/movim/workers/sessionsworker.php' 2>/dev/null || true
pkill -f '[m]ovim-testbed/movim/workers/galener.php' 2>/dev/null || true
pkill -f '[g]alene -insecure -http localhost:18444' 2>/dev/null || true

echo "Movim testbed stopped. Prosody was left running."
