#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

unset http_proxy https_proxy all_proxy HTTP_PROXY HTTPS_PROXY ALL_PROXY
export NO_PROXY="127.0.0.1,localhost,::1"
export no_proxy="$NO_PROXY"

export APP_ENV="${APP_ENV:-local}"
export SERVICE_NAME="${SERVICE_NAME:-lenso-first-user-smoke}"
export LENSO_COMPOSITION_PROFILE="${LENSO_COMPOSITION_PROFILE:-demo}"
export DATABASE_URL="${DATABASE_URL:-postgres://lenso:lenso@localhost:5432/lenso}"
export DATABASE_MAX_CONNECTIONS="${DATABASE_MAX_CONNECTIONS:-10}"
export HTTP_HOST="${HTTP_HOST:-127.0.0.1}"
export HTTP_PORT="${FIRST_USER_SMOKE_HTTP_PORT:-3300}"
export RUST_LOG="${RUST_LOG:-info,lenso=debug}"
export LOG_FORMAT="${LOG_FORMAT:-compact}"
export REMOTE_MODULE_ADDR="${FIRST_USER_SMOKE_REMOTE_MODULE_ADDR:-127.0.0.1:4107}"
export REMOTE_MODULES="remote-crm=http://${REMOTE_MODULE_ADDR}/lenso/module/v1"

remote_log="$repo_root/target/first-user-remote.log"
api_log="$repo_root/target/first-user-api.log"
worker_log="$repo_root/target/first-user-worker.log"
api_url="http://127.0.0.1:${HTTP_PORT}"
remote_url="http://${REMOTE_MODULE_ADDR}"

wait_url() {
    url="$1"
    label="$2"
    for _ in $(seq 1 45); do
        if curl --noproxy '*' -fsS "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    echo "Timed out waiting for $label at $url" >&2
    return 1
}

cleanup() {
    status=$?
    if [ "$status" -ne 0 ]; then
        for log in "$remote_log" "$api_log" "$worker_log"; do
            if [ -f "$log" ]; then
                echo "---- ${log#$repo_root/} ----" >&2
                tail -80 "$log" >&2 || true
            fi
        done
    fi
    kill "${remote_pid:-}" "${api_pid:-}" "${worker_pid:-}" 2>/dev/null || true
    exit "$status"
}
trap cleanup EXIT

mkdir -p "$repo_root/target"

if curl --noproxy '*' -fsS "$api_url/livez" >/dev/null 2>&1; then
    echo "Port $HTTP_PORT already serves a Lenso API; stop it or set FIRST_USER_SMOKE_HTTP_PORT." >&2
    exit 1
fi

just --justfile "$repo_root/justfile" db-up
just --justfile "$repo_root/justfile" migrate

cargo run --locked -p remote-module-fixture >"$remote_log" 2>&1 &
remote_pid=$!
wait_url "$remote_url/lenso/module/v1/manifest" "remote module"

cargo run --locked -p app-api >"$api_log" 2>&1 &
api_pid=$!
wait_url "$api_url/livez" "API"

cargo run --locked -p app-worker >"$worker_log" 2>&1 &
worker_pid=$!

curl --noproxy '*' -fsS \
    -H "Authorization: Bearer dev-service:admin:remote_crm.contacts.read" \
    -H "x-correlation-id: corr_first_user_smoke" \
    "$api_url/modules/remote-crm/http/contacts/contact_1" >/dev/null

curl --noproxy '*' -fsS \
    -H "Authorization: Bearer dev-service:admin" \
    "$api_url/admin/data/modules" | grep -q "remote-crm"

curl --noproxy '*' -fsS \
    -H "Authorization: Bearer dev-service:admin" \
    "$api_url/admin/runtime/remote-proxy-calls?correlation_id=corr_first_user_smoke&limit=10" \
    | grep -q "corr_first_user_smoke"

echo "First-user smoke passed."
