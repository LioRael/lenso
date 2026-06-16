#!/usr/bin/env sh
set -eu

api_base="${VITE_API_BASE_URL:-${API_BASE_URL:-http://localhost:3000}}"
module_name="${RUNTIME_CONSOLE_REMOTE_FIXTURE_MODULE:-remote-crm}"
contact_id="${RUNTIME_CONSOLE_REMOTE_FIXTURE_CONTACT:-contact_1}"
token="${RUNTIME_CONSOLE_REMOTE_FIXTURE_TOKEN:-dev-service:admin:remote_crm.contacts.read}"
stamp="$(date -u +%Y%m%d%H%M%S)"
request_id="${RUNTIME_CONSOLE_REMOTE_FIXTURE_REQUEST_ID:-req_console_api_fixture_${stamp}_$$}"
correlation_id="${RUNTIME_CONSOLE_REMOTE_FIXTURE_CORRELATION_ID:-corr_console_api_fixture}"
path="/modules/$module_name/http/contacts/$contact_id"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1" >&2
        exit 1
    fi
}

require_cmd curl
require_cmd jq

tmp_response="$(mktemp)"
cleanup() {
    rm -f "$tmp_response"
}
trap cleanup EXIT

echo "Runtime Console API fixture: $module_name $contact_id"

status="$(
    curl --noproxy "*" -sS -o "$tmp_response" -w "%{http_code}" \
        -H "Authorization: Bearer $token" \
        -H "x-request-id: $request_id" \
        -H "x-correlation-id: $correlation_id" \
        "$api_base$path" || true
)"

if [ "$status" != "200" ]; then
    cat >&2 <<EOF
Runtime Console API fixture could not create the remote proxy sample.

Request:
  GET $api_base$path

HTTP status:
  $status

Response:
EOF
    jq . "$tmp_response" >&2 2>/dev/null || cat "$tmp_response" >&2
    cat >&2 <<EOF

Start the remote module fixture and API with remote-crm loaded:
  cargo run --locked -p remote-module-example
  REMOTE_MODULES=remote-crm=http://127.0.0.1:4100/lenso/module/v1 just api

Then run:
  just console-api-qa
EOF
    exit 1
fi

if ! jq -e \
    --arg module_name "$module_name" \
    --arg contact_id "$contact_id" \
    '.status == "forwarded" and .module_name == $module_name and .data.id == $contact_id' \
    "$tmp_response" >/dev/null; then
    echo "Runtime Console API fixture response shape is unexpected." >&2
    jq . "$tmp_response" >&2 || cat "$tmp_response" >&2
    exit 1
fi

echo "Runtime Console API fixture created:"
echo "- request_id=$request_id"
echo "- correlation_id=$correlation_id"
