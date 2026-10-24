#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$ROOT/../.." && pwd)"
JABU_DIR="$ROOT/movim"
PGDATA="$ROOT/postgres"
LOGS="$ROOT/logs"
PGPORT="${JABU_TESTBED_PGPORT:-55432}"

if [ ! -d "$JABU_DIR/.git" ]; then
    echo "Run $ROOT/setup.sh first (inside nix-shell $ROOT/shell.nix)." >&2
    exit 1
fi

mkdir -p "$LOGS"

echo "== PostgreSQL =="
if ! pg_ctl -D "$PGDATA" status >/dev/null 2>&1; then
    pg_ctl -D "$PGDATA" -l "$LOGS/postgres.log" \
        -o "-p $PGPORT -k $PGDATA -c listen_addresses=127.0.0.1" start
fi
for _ in $(seq 1 40); do
    pg_isready -h 127.0.0.1 -p "$PGPORT" -U movim >/dev/null 2>&1 && break
    sleep 0.25
done

echo "== Prosody =="
if ! (exec 3<>/dev/tcp/127.0.0.1/5222) 2>/dev/null; then
    "$REPO/prosody/run.sh"
else
    exec 3>&- 3<&- 2>/dev/null || true
fi
export JABU_PROSODY_DIR="$REPO/prosody"
prosodyctl --config "$REPO/prosody/config/prosody.cfg.lua" \
    register jabu localhost jabu-test >/dev/null 2>&1 || true

echo "== Movim daemon =="
if (exec 3<>/dev/tcp/127.0.0.1/8081) 2>/dev/null || { [ -f "$LOGS/daemon.pid" ] && kill -0 "$(cat "$LOGS/daemon.pid")" 2>/dev/null; }; then
    exec 3>&- 3<&- 2>/dev/null || true
    echo "daemon already running"
else
    cd "$JABU_DIR"
    setsid php -d opcache.enable_cli=1 -d display_errors=0 -d error_reporting=8191 -d log_errors=1 \
        daemon.php start > "$LOGS/daemon.log" 2>&1 < /dev/null &
    echo $! > "$LOGS/daemon.pid"
    for _ in $(seq 1 60); do
        [ -S "$JABU_DIR/cache/socketapi.sock" ] && break
        sleep 0.25
    done
fi

echo "== Galener worker =="
if [ -S "$JABU_DIR/cache/socketapi.sock" ]; then
    (cd "$JABU_DIR" && php daemon.php galener start) || true
else
    echo "daemon API socket not ready; start Galener later with:" >&2
    echo "  (cd $JABU_DIR && php daemon.php galener start)" >&2
fi

echo "== Movim web =="
NGINX_CONF="$(cd "$(dirname "$(command -v nginx)")/.." && pwd)/conf"
sed -e "s|@LOGS@|$LOGS|g" -e "s|@PUBLIC@|$JABU_DIR/public|g" -e "s|@NGINX_CONF@|$NGINX_CONF|g" \
    "$ROOT/nginx.conf.template" > "$LOGS/nginx.conf"
sed -e "s|@LOGS@|$LOGS|g" "$ROOT/php-fpm.conf.template" > "$LOGS/php-fpm.conf"
mkdir -p "$LOGS/nginx-body" "$LOGS/nginx-proxy" "$LOGS/nginx-fastcgi"

if ! (exec 3<>/dev/tcp/127.0.0.1/9000) 2>/dev/null; then
    setsid php-fpm -y "$LOGS/php-fpm.conf" > "$LOGS/php-fpm.out" 2>&1 < /dev/null &
    for _ in $(seq 1 40); do
        (exec 3<>/dev/tcp/127.0.0.1/9000) 2>/dev/null && break
        sleep 0.25
    done
    exec 3>&- 3<&- 2>/dev/null || true
fi

if ! (exec 3<>/dev/tcp/127.0.0.1/8090) 2>/dev/null; then
    setsid nginx -c "$LOGS/nginx.conf" -g 'daemon off;' > "$LOGS/web.log" 2>&1 < /dev/null &
fi

cat <<EOF

Movim testbed is running.

  Open:        http://127.0.0.1:8090/
  Log in as:   jabu@localhost / jabu-test
  Daemon API:  127.0.0.1:8081
  Galener:     sfu.example.org (component port 5347)

First login must be done in the browser (Movim logs in over the daemon
WebSocket). After that, create or join the room voice@conference.localhost,
open the room configuration and invite the SFU component.

Logs:
  $LOGS/daemon.log
  $LOGS/web.log
  $LOGS/postgres.log

Stop with: $ROOT/stop.sh
EOF
