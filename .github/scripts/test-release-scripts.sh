#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
GATE="$ROOT/.github/scripts/release-gate.sh"
PLAN="$ROOT/.github/scripts/release-plan.sh"
current_sha="$(git -C "$ROOT" rev-parse HEAD)"
mock_dir="$(mktemp -d)"
test_dir="$(mktemp -d)"
test_repo="$test_dir/repo"
git clone --no-local "$ROOT" "$test_repo" >/dev/null
trap 'rm -rf "$mock_dir" "$test_dir"' EXIT

base_env=(
  "GITHUB_REPOSITORY=LioRael/lenso"
  "GITHUB_REF=refs/heads/main"
  "GITHUB_EVENT_NAME=workflow_dispatch"
  "GITHUB_TOKEN=test-token"
  "RELEASE_SET=[]"
  "RELEASE_MODE=dry-run"
)

run_gate() {
  (
    cd "$test_repo"
    env "$@" bash "$GATE"
  )
}

expect_failure() {
  local label="$1"
  local expected="$2"
  shift 2
  local output
  if output="$("$@" 2>&1)"; then
    printf 'expected failure did not occur: %s\n' "$label" >&2
    exit 1
  fi
  if [[ "$output" != *"$expected"* ]]; then
    printf 'failure for %s did not contain %s:\n%s\n' "$label" "$expected" "$output" >&2
    exit 1
  fi
  printf 'rejected as expected: %s\n' "$label"
}

expect_failure "invalid full SHA" "full 40-character" \
  run_gate "${base_env[@]}" RELEASE_SHA=not-a-sha

unlanded_sha="$(
  git -C "$test_repo" \
    -c user.name='Release gate test' \
    -c user.email='release-gate-test@example.invalid' \
    commit-tree "$(git -C "$test_repo" rev-parse HEAD^{tree})" -p "$current_sha" \
    <<<"unlanded release gate test"
)"

cat >"$mock_dir/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

args="$*"
sha="${MOCK_SHA:?MOCK_SHA is required}"
run_conclusion="${MOCK_RUN_CONCLUSION:-failure}"
job_conclusion="${MOCK_JOB_CONCLUSION:-failure}"
job_sha="${MOCK_JOB_SHA:-$sha}"
job_attempt="${MOCK_JOB_ATTEMPT:-1}"
if [[ "$args" == *"actions/workflows/ci.yml"* ]]; then
  if [[ "$args" == *"--jq"* ]]; then
    printf '294726715\n'
  else
    printf '{"id":294726715}\n'
  fi
elif [[ "$args" == *"git/ref/heads/main"* ]]; then
  if [[ "$args" == *"--jq"* ]]; then
    printf '%s\n' "$sha"
  else
    printf '{"object":{"sha":"%s"}}\n' "$sha"
  fi
elif [[ "$args" == *"/jobs?"* ]]; then
  printf '[{"jobs":[{"name":"quality","head_sha":"%s","run_attempt":%s,"status":"completed","conclusion":"%s"}]}]\n' \
    "$job_sha" "$job_attempt" "$job_conclusion"
elif [[ "$args" == *"actions/runs?head_sha="* ]]; then
  printf '[{"workflow_runs":[{"id":999,"workflow_id":294726715,"name":"CI","path":".github/workflows/ci.yml","event":"push","status":"completed","conclusion":"%s","head_branch":"delta/verify/test/1","head_sha":"%s","run_attempt":1,"html_url":"https://example.invalid/run/999"}]}]\n' \
    "$run_conclusion" "$sha"
else
  printf 'unexpected gh api request: %s\n' "$args" >&2
  exit 2
fi
EOF
cat >"$mock_dir/curl" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "${MOCK_CURL_STATUS:-200}"
EOF
chmod +x "$mock_dir/gh" "$mock_dir/curl"

git -C "$test_repo" switch --detach "$unlanded_sha" >/dev/null
expect_failure "unlanded SHA" "not reachable from the current origin/main" \
  run_gate "${base_env[@]}" RELEASE_SHA="$unlanded_sha" PATH="$mock_dir:$PATH" MOCK_SHA="$current_sha"
git -C "$test_repo" switch main >/dev/null

expect_failure "CI quality failure for otherwise landed SHA" \
  "no successful candidate push CI run" \
  run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" PATH="$mock_dir:$PATH" MOCK_SHA="$current_sha"

expect_failure "CI job from a different attempt" \
  "does not contain one successful quality job" \
  run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" PATH="$mock_dir:$PATH" \
  MOCK_SHA="$current_sha" MOCK_RUN_CONCLUSION=success MOCK_JOB_CONCLUSION=success MOCK_JOB_ATTEMPT=2

all_packages='[{"package_name":"lenso-app-plan","version":"0.4.3"},{"package_name":"lenso-kernel","version":"0.3.9"},{"package_name":"lenso-runtime-conformance","version":"0.3.8"}]'
expect_failure "registry release-set mismatch" "read-only crates.io plan" \
  run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" RELEASE_SET="$all_packages" \
  PATH="$mock_dir:$PATH" MOCK_SHA="$current_sha"

expect_failure "registry error is not treated as absence" "unexpected crates.io response 500" \
  run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" PATH="$mock_dir:$PATH" \
  MOCK_SHA="$current_sha" MOCK_CURL_STATUS=500

env EXPECTED_RELEASE_SET='[{"package_name":"lenso-kernel","version":"0.3.10"}]' \
  ACTUAL_RELEASES=null bash "$PLAN"
expect_failure "dry-run release record mismatch" "unexpected release set" \
  env EXPECTED_RELEASE_SET='[]' \
    ACTUAL_RELEASES='[{"package_name":"lenso-kernel","version":"0.3.9"}]' \
    bash "$PLAN"

printf '%s\n' 'release script negative and dry-run plan tests passed'
