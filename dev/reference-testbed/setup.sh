#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JABU_DIR="$ROOT/movim"
PGDATA="$ROOT/postgres"
LOGS="$ROOT/logs"
PGPORT="${JABU_TESTBED_PGPORT:-55432}"
GALENER_SECRET="${GALENER_SECRET:-jabu-galener-test}"

mkdir -p "$PGDATA" "$LOGS"

composer_run() {
    local bin root
    bin="$(readlink -f "$(command -v composer)")"
    root="$(cd "$(dirname "$bin")/.." && pwd)"
    php "$root/share/php/composer/bin/composer" "$@"
}

for tool in php composer galene initdb pg_ctl pg_isready createdb psql git; do
    command -v "$tool" >/dev/null || {
        echo "Missing $tool. Run this script inside: nix-shell $ROOT/shell.nix" >&2
        exit 1
    }
done

echo "== PostgreSQL =="
if [ ! -f "$PGDATA/PG_VERSION" ]; then
    initdb -D "$PGDATA" -U movim --auth=trust --encoding=UTF8 --locale=C
fi
if ! pg_ctl -D "$PGDATA" status >/dev/null 2>&1; then
    pg_ctl -D "$PGDATA" -l "$LOGS/postgres.log" \
        -o "-p $PGPORT -k $PGDATA -c listen_addresses=127.0.0.1" start
fi
for _ in $(seq 1 40); do
    pg_isready -h 127.0.0.1 -p "$PGPORT" -U movim >/dev/null 2>&1 && break
    sleep 0.25
done
if ! psql -h 127.0.0.1 -p "$PGPORT" -U movim -d postgres -tAc \
    "SELECT 1 FROM pg_database WHERE datname = 'movim'" | grep -q 1; then
    createdb -h 127.0.0.1 -p "$PGPORT" -U movim movim
fi

echo "== Movim =="
if [ ! -d "$JABU_DIR/.git" ]; then
    git clone --depth 1 https://github.com/movim/movim.git "$JABU_DIR"
fi

# Testbed-only patches:
#  0001 - let the session worker inherit Nix's php.ini
#  0002 - Galener/Prosody compatibility and uninitialised typed-property fixes
for patch in "$ROOT"/patches/*.patch; do
    if git -C "$JABU_DIR" apply --check "$patch" 2>/dev/null; then
        git -C "$JABU_DIR" apply "$patch"
        echo "applied $(basename "$patch")"
    elif git -C "$JABU_DIR" apply --reverse --check "$patch" 2>/dev/null; then
        echo "already applied $(basename "$patch")"
    else
        echo "Patch does not apply cleanly: $patch" >&2
        exit 1
    fi
done

mkdir -p "$JABU_DIR/cache" "$JABU_DIR/log" "$JABU_DIR/public/cache"
chmod u+rwx "$JABU_DIR/cache" "$JABU_DIR/log" "$JABU_DIR/public/cache"

cat > "$JABU_DIR/.env" <<EOF
DB_DRIVER=pgsql
DB_HOST=127.0.0.1
DB_PORT=$PGPORT
DB_DATABASE=movim
DB_USERNAME=movim
DB_PASSWORD=movim

DAEMON_URL=http://127.0.0.1:8090/
DAEMON_PORT=8081
DAEMON_INTERFACE=127.0.0.1
DAEMON_DEBUG=false
DAEMON_VERBOSE=true

GALENER_XMPP_IP=127.0.0.1
GALENER_XMPP_PORT=5347
GALENER_XMPP_HOST=sfu.example.org
GALENER_XMPP_PASSWORD=$GALENER_SECRET
GALENER_GALENE_PATH=$(command -v galene)
EOF

cd "$JABU_DIR"
COMPOSER_MEMORY_LIMIT=-1 composer_run install --no-interaction --no-progress --prefer-dist
COMPOSER_MEMORY_LIMIT=-1 composer_run movim:migrate

echo
echo "Movim testbed is ready."
echo "  Movim web:      http://127.0.0.1:8090/"
echo "  XMPP account:   jabu@localhost / jabu-test"
echo "  Galene:         $(command -v galene)"
echo "  PostgreSQL:     127.0.0.1:$PGPORT (database movim)"
echo
echo "Start everything with: $ROOT/run.sh"
