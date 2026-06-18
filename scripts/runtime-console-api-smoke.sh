#!/usr/bin/env sh
set -eu

api_base="${VITE_API_BASE_URL:-${API_BASE_URL:-http://localhost:3000}}"
token="${RUNTIME_CONSOLE_TOKEN:-dev-service:admin}"
auth_header="Authorization: Bearer $token"
remote_fixture_correlation_id="${RUNTIME_CONSOLE_REMOTE_FIXTURE_CORRELATION_ID:-corr_console_api_fixture}"
remote_runtime_function_name="${RUNTIME_CONSOLE_REMOTE_FUNCTION_NAME:-remote_crm.sync_contact.v1}"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1" >&2
        exit 1
    fi
}

api_get() {
    path="$1"
    curl --noproxy "*" -fsS -H "$auth_header" "$api_base$path"
}

assert_json() {
    file="$1"
    expr="$2"
    message="$3"
    if ! jq -e "$expr" "$file" >/dev/null; then
        echo "Runtime Console API smoke failed: $message" >&2
        echo "Response:" >&2
        jq . "$file" >&2 || cat "$file" >&2
        exit 1
    fi
}

assert_jq() {
    file="$1"
    message="$2"
    shift 2
    if ! jq -e "$@" "$file" >/dev/null; then
        echo "Runtime Console API smoke failed: $message" >&2
        echo "Response:" >&2
        jq . "$file" >&2 || cat "$file" >&2
        exit 1
    fi
}

wait_for_remote_runtime_function() {
    file="$1"
    for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30; do
        api_get "/admin/runtime/functions?function_name=$remote_runtime_function_name&limit=10" >"$file"
        if jq -e '[.data[] | select(.status == "completed")] | length > 0' "$file" >/dev/null; then
            return 0
        fi
        if jq -e '[.data[] | select(.status == "dead")] | length > 0' "$file" >/dev/null; then
            break
        fi
        sleep 1
    done

    echo "Runtime Console API smoke failed: remote runtime function did not complete" >&2
    echo "Start lenso-worker with REMOTE_MODULES configured." >&2
    echo "Response:" >&2
    jq . "$file" >&2 || cat "$file" >&2
    exit 1
}

tmpdir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

require_cmd curl
require_cmd jq

echo "Runtime Console API smoke: $api_base"

summary="$tmpdir/summary.json"
api_get "/admin/runtime/summary" >"$summary"
assert_json "$summary" '.status | type == "string"' "summary status is missing"
assert_json "$summary" '.outbox.pending | type == "number"' "summary outbox counts are missing"
assert_json "$summary" '.functions.completed | type == "number"' "summary function counts are missing"
assert_json "$summary" '.recent_activity | type == "array"' "summary recent activity is missing"

outbox="$tmpdir/outbox.json"
api_get "/admin/runtime/outbox?limit=1" >"$outbox"
assert_json "$outbox" '.data | type == "array"' "outbox list data is missing"
assert_json "$outbox" '.page.limit == 1' "outbox list did not preserve limit"
if [ "$(jq '.data | length' "$outbox")" -gt 0 ]; then
    outbox_id="$(jq -r '.data[0].id' "$outbox")"
    outbox_detail="$tmpdir/outbox-detail.json"
    api_get "/admin/runtime/outbox/$outbox_id" >"$outbox_detail"
    assert_json "$outbox_detail" '.data.id == "'"$outbox_id"'"' "outbox detail id mismatch"
    assert_json "$outbox_detail" '.data.payload != null' "outbox detail payload is missing"
    assert_json "$outbox_detail" '.data.actor | type == "object"' "outbox detail actor is missing"
fi

functions="$tmpdir/functions.json"
api_get "/admin/runtime/functions?limit=1" >"$functions"
assert_json "$functions" '.data | type == "array"' "function list data is missing"
assert_json "$functions" '.page.limit == 1' "function list did not preserve limit"
if [ "$(jq '.data | length' "$functions")" -gt 0 ]; then
    function_id="$(jq -r '.data[0].id' "$functions")"
    function_detail="$tmpdir/function-detail.json"
    api_get "/admin/runtime/functions/$function_id" >"$function_detail"
    assert_json "$function_detail" '.data.id == "'"$function_id"'"' "function detail id mismatch"
    assert_json "$function_detail" '.data.actor | type == "object"' "function detail actor is missing"
    assert_json "$function_detail" 'has("data") and (.data | has("runtime_declaration"))' "function declaration field is missing"
fi

remote_calls="$tmpdir/remote-calls.json"
api_get "/admin/runtime/remote-proxy-calls?correlation_id=$remote_fixture_correlation_id&limit=1" >"$remote_calls"
if [ "$(jq '.data | length' "$remote_calls")" -eq 0 ]; then
    api_get "/admin/runtime/remote-proxy-calls?limit=1" >"$remote_calls"
fi
assert_json "$remote_calls" '.data | type == "array"' "remote call list data is missing"
assert_json "$remote_calls" '.page.limit == 1' "remote call list did not preserve limit"
if [ "$(jq '.data | length' "$remote_calls")" -gt 0 ]; then
    remote_call_id="$(jq -r '.data[0].id' "$remote_calls")"
    correlation_id="$(jq -r '.data[0].correlation_id' "$remote_calls")"
    module_name="$(jq -r '.data[0].module_name' "$remote_calls")"
    success="$(jq -r '.data[0].success' "$remote_calls")"
    remote_node_id="remoteproxy_$remote_call_id"
    filtered_remote_calls="$tmpdir/remote-calls-filtered.json"
    api_get "/admin/runtime/remote-proxy-calls?correlation_id=$correlation_id&module_name=$module_name&success=$success&limit=10" >"$filtered_remote_calls"
    assert_json "$filtered_remote_calls" '.data | type == "array"' "remote call filtered data is missing"
    if ! jq -e \
        --arg correlation_id "$correlation_id" \
        --arg module_name "$module_name" \
        --argjson success "$success" \
        'all(.data[]; .correlation_id == $correlation_id and .module_name == $module_name and .success == $success)' \
        "$filtered_remote_calls" >/dev/null; then
        echo "Runtime Console API smoke failed: remote call filters are not preserved" >&2
        echo "Response:" >&2
        jq . "$filtered_remote_calls" >&2 || cat "$filtered_remote_calls" >&2
        exit 1
    fi

    next_created_before="$(jq -r '.page.next_created_before // empty' "$remote_calls")"
    if [ -n "$next_created_before" ]; then
        paged_remote_calls="$tmpdir/remote-calls-paged.json"
        api_get "/admin/runtime/remote-proxy-calls?created_before=$next_created_before&limit=1" >"$paged_remote_calls"
        assert_json "$paged_remote_calls" '.page.limit == 1' "remote call pagination limit is missing"
    fi

    story="$tmpdir/remote-story.json"
    api_get "/admin/runtime/stories/$correlation_id" >"$story"
    assert_json "$story" '.data.summary.correlation_id == "'"$correlation_id"'"' "remote call story correlation mismatch"
    assert_jq "$story" "remote proxy call story node is missing" \
        --arg node_id "$remote_node_id" '
        any(.data.nodes[]; .id == $node_id and .type == "remote_proxy_call")
    '
    assert_jq "$story" "remote proxy call story node metadata is incomplete" \
        --arg node_id "$remote_node_id" \
        --arg call_id "$remote_call_id" \
        --arg module_name "$module_name" '
        any(.data.nodes[];
            .id == $node_id
            and .metadata.source_metadata.remote_proxy_call_id == $call_id
            and .metadata.source_metadata.module_name == $module_name
        )
    '
    assert_jq "$story" "remote proxy call timeline item is missing" \
        --arg node_id "$remote_node_id" '
        any(.data.timeline_items[]; .related_node_id == $node_id and .type == "remote_proxy_call")
    '

    story_operations="$tmpdir/remote-story-operations.json"
    api_get "/admin/runtime/stories/$correlation_id/technical-operations" >"$story_operations"
    assert_jq "$story_operations" "remote proxy technical operation is missing" \
        --arg node_id "$remote_node_id" \
        --arg call_id "$remote_call_id" '
        any(.data[];
            .related_node_id == $node_id
            and .source == "remote_proxy"
            and .id == ("remote_proxy:" + $call_id)
        )
    '

    payload="$tmpdir/remote-payload.json"
    api_get "/admin/runtime/executions/$remote_node_id/payload" >"$payload"
    assert_json "$payload" '.data.node_type == "story_event"' "remote proxy payload node type is unexpected"
    assert_jq "$payload" "remote proxy payload metadata is incomplete" \
        --arg call_id "$remote_call_id" \
        --arg module_name "$module_name" '
        .data.input.remote_proxy_call_id == $call_id
        and .data.input.module_name == $module_name
        and .data.metadata.node_type == "remote_proxy_call"
    '

    logs="$tmpdir/remote-logs.json"
    api_get "/admin/runtime/executions/$remote_node_id/logs" >"$logs"
    assert_json "$logs" '.data | type == "array"' "remote proxy logs response data is missing"
    assert_json "$logs" '.page.limit | type == "number"' "remote proxy logs page metadata is missing"
fi

remote_functions="$tmpdir/remote-functions.json"
wait_for_remote_runtime_function "$remote_functions"
assert_jq "$remote_functions" "remote runtime function list item is missing declaration metadata" \
    --arg function_name "$remote_runtime_function_name" '
    any(.data[];
        .function_name == $function_name
        and .status == "completed"
        and .runtime_declaration.module_name == "remote-crm"
        and .runtime_declaration.module_source == "remote"
        and .runtime_declaration.queue == "remote-crm"
        and .runtime_declaration.input_schema == $function_name
    )
'
remote_function_id="$(jq -r --arg function_name "$remote_runtime_function_name" '[.data[] | select(.function_name == $function_name and .status == "completed")][0].id' "$remote_functions")"
remote_function_correlation_id="$(jq -r --arg id "$remote_function_id" '.data[] | select(.id == $id) | .correlation_id' "$remote_functions")"

remote_function_detail="$tmpdir/remote-function-detail.json"
api_get "/admin/runtime/functions/$remote_function_id" >"$remote_function_detail"
assert_jq "$remote_function_detail" "remote runtime function detail is incomplete" \
    --arg id "$remote_function_id" \
    --arg function_name "$remote_runtime_function_name" '
    .data.id == $id
    and .data.function_name == $function_name
    and .data.status == "completed"
    and .data.runtime_declaration.module_source == "remote"
    and .data.input_json.reason == "worker_startup"
'

remote_function_story="$tmpdir/remote-function-story.json"
api_get "/admin/runtime/stories/$remote_function_correlation_id" >"$remote_function_story"
assert_jq "$remote_function_story" "remote runtime function story node is missing" \
    --arg id "$remote_function_id" \
    --arg function_name "$remote_runtime_function_name" '
    any(.data.nodes[]; .id == $id and .type == "function" and .name == $function_name and .status == "completed")
    and any(.data.timeline_items[]; .related_node_id == $id and .type == "function_run")
'

remote_function_operations="$tmpdir/remote-function-operations.json"
api_get "/admin/runtime/executions/$remote_function_id/technical-operations" >"$remote_function_operations"
assert_jq "$remote_function_operations" "remote runtime technical operation is missing" \
    --arg id "$remote_function_id" \
    --arg function_name "$remote_runtime_function_name" '
    any(.data[];
        .related_node_id == $id
        and .source == "remote_runtime"
        and .status == "ok"
        and .attributes.function_name == $function_name
        and .attributes.module_name == "remote-crm"
        and .attributes.success == true
    )
'

remote_function_logs="$tmpdir/remote-function-logs.json"
api_get "/admin/runtime/executions/$remote_function_id/logs" >"$remote_function_logs"
assert_jq "$remote_function_logs" "remote runtime function logs are missing lifecycle entries" \
    --arg id "$remote_function_id" '
    (.page.limit | type == "number")
    and any(.data[]; .node_id == $id and .body == "Function run started")
    and any(.data[]; .node_id == $id and .body == "Function handler operation completed" and .attributes.source == "remote_runtime")
    and any(.data[]; .node_id == $id and .body == "Function run completed")
'

echo "Runtime Console API smoke passed."
echo "- summary supports queue pressure inputs"
echo "- outbox list/detail supports dead-letter inspector payload and actor"
echo "- functions list/detail supports operation inspector metadata"
echo "- remote calls support filters and pagination"
echo "- remote call stories expose nodes, technical operations, payloads, and logs"
echo "- remote runtime functions complete with story nodes, operations, and logs"
