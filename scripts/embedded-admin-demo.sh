#!/usr/bin/env sh
set -eu

remote_addr="${REMOTE_MODULE_ADDR:-127.0.0.1:4100}"
api_host="${HTTP_HOST:-0.0.0.0}"
api_port="${HTTP_PORT:-3000}"
api_base="${VITE_API_BASE_URL:-http://localhost:$api_port}"
console_port="${CONSOLE_PORT:-5174}"
console_url="http://localhost:$console_port/data"

pids=""

cleanup() {
    for pid in $pids; do
        kill "$pid" 2>/dev/null || true
    done
}

trap cleanup INT TERM EXIT

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

assert_port_free "$(port_from_addr "$remote_addr")" "Remote module"
assert_port_free "$api_port" "API"
assert_port_free "$console_port" "Runtime Console"

start_bg env REMOTE_MODULE_ADDR="$remote_addr" cargo run --locked -p remote-module-example

remote_base="http://$remote_addr/lenso/module/v1"
remote_modules="remote-crm=$remote_base,remote-crm-embedded=$remote_base/embedded"

start_bg env HTTP_HOST="$api_host" HTTP_PORT="$api_port" REMOTE_MODULES="$remote_modules" cargo run --locked -p app-api
start_bg env VITE_RUNTIME_CONSOLE_MODE=api VITE_API_BASE_URL="$api_base" pnpm --dir=apps/runtime-console exec vite --host 0.0.0.0 --port "$console_port" --strictPort

cat <<EOF

Embedded admin demo is starting.

Remote module:  http://$remote_addr
API base:       $api_base
Console page:   $console_url

Open the Console Data page and select "remote-crm-embedded".
Press Ctrl-C here to stop all demo processes.

EOF

wait
