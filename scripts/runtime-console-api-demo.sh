#!/usr/bin/env sh
set -eu

remote_addr="${REMOTE_MODULE_ADDR:-127.0.0.1:4100}"
api_host="${HTTP_HOST:-0.0.0.0}"
api_port="${HTTP_PORT:-3000}"
api_base="${VITE_API_BASE_URL:-http://localhost:$api_port}"
console_port="${CONSOLE_PORT:-5174}"
console_url="http://localhost:$console_port/operations/remote-calls"
runtime_console_dir="${RUNTIME_CONSOLE_DIR:-../lenso-runtime-console}"
skip_db_setup="${SKIP_DB_SETUP:-0}"

pids=""

cleanup() {
    for pid in $pids; do
        kill "$pid" 2>/dev/null || true
    done
}

trap cleanup INT TERM EXIT

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1" >&2
        exit 1
    fi
}

port_from_addr() {
    printf '%s' "$1" | awk -F: '{print $NF}'
}

assert_port_free() {
    port="$1"
    label="$2"
    if command -v lsof >/dev/null 2>&1 && lsof -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
        echo "$label port $port is already in use." >&2
        echo "Override the port or stop the process using it." >&2
        exit 1
    fi
}

start_bg() {
    "$@" &
    pid=$!
    pids="$pids $pid"
}

wait_for_url() {
    url="$1"
    label="$2"
    for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
        if curl --noproxy "*" -fsS "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    echo "$label did not become ready: $url" >&2
    exit 1
}

wait_for_health() {
    url="$1"
    label="$2"
    for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
        if curl --noproxy "*" -fsS "$url" 2>/dev/null | jq -e '.status == "healthy"' >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    echo "$label did not become healthy: $url" >&2
    exit 1
}

require_cmd cargo
require_cmd curl
require_cmd jq
require_cmd just
require_cmd pnpm

if [ ! -f "$runtime_console_dir/package.json" ]; then
    echo "Runtime Console repository not found: $runtime_console_dir" >&2
    echo "Set RUNTIME_CONSOLE_DIR to the lenso-runtime-console checkout." >&2
    exit 1
fi

assert_port_free "$(port_from_addr "$remote_addr")" "Remote module"
assert_port_free "$api_port" "API"
assert_port_free "$console_port" "Runtime Console"

if [ "$skip_db_setup" != "1" ]; then
    echo "Starting local Postgres and running migrations..."
    if ! just db-up; then
        cat >&2 <<EOF
Could not start local Postgres.

Start Docker/OrbStack and rerun:
  just console-api-demo

Or, if Postgres is already available:
  SKIP_DB_SETUP=1 just console-api-demo
EOF
        exit 1
    fi
    just migrate
fi

start_bg env REMOTE_MODULE_ADDR="$remote_addr" cargo run --locked -p remote-module-example

remote_base="http://$remote_addr/lenso/module/v1"
remote_manifest="$remote_base/manifest"
wait_for_url "$remote_manifest" "Remote module"

remote_modules="remote-crm=$remote_base,remote-crm-embedded=$remote_base/embedded,remote-crm-declarative=$remote_base/declarative"

start_bg env HTTP_HOST="$api_host" HTTP_PORT="$api_port" REMOTE_MODULES="$remote_modules" cargo run --locked -p app-api
wait_for_health "$api_base/readyz" "API"

start_bg env REMOTE_MODULES="$remote_modules" cargo run --locked -p app-worker

start_bg env VITE_RUNTIME_CONSOLE_MODE=api VITE_API_BASE_URL="$api_base" pnpm --dir="$runtime_console_dir" exec vite --host 0.0.0.0 --port "$console_port" --strictPort
wait_for_url "http://localhost:$console_port/" "Runtime Console"

cat <<EOF

Runtime Console API demo is running.

Remote module:  http://$remote_addr
API base:       $api_base
Console page:   $console_url
Worker:         app-worker with remote runtime functions and event handlers

Verify the seeded remote story and runtime function paths:
  just console-api-qa

Open the Console and check:
  /operations/remote-calls
  /runtime/stories
  /operations/functions
  /operations/dead-letters
  /operations/queues

Press Ctrl-C here to stop the demo processes.

EOF

wait
