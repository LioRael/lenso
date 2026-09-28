#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
GATE="$ROOT/.github/scripts/release-gate.sh"
PLAN="$ROOT/.github/scripts/release-plan.sh"
STAGE_CONFIG="$ROOT/.github/scripts/release-stage-config.sh"
VERIFY_STAGE="$ROOT/.github/scripts/verify-release-stage-output.sh"
current_sha="$(git -C "$ROOT" rev-parse HEAD)"
app_plan_version="$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --locked --no-deps --format-version 1 |
  jq -r '.packages[] | select(.name == "lenso-app-plan") | .version')"
stage_set="$(jq -cn --arg version "$app_plan_version" '[{package_name:"lenso-app-plan", version:$version}]')"
mock_dir="$(mktemp -d)"
test_dir="$(mktemp -d)"
test_remote="$test_dir/origin.git"
test_repo="$test_dir/repo"
git init --bare "$test_remote" >/dev/null
git -C "$ROOT" push "$test_remote" "$current_sha:refs/heads/main" >/dev/null
git -C "$test_remote" symbolic-ref HEAD refs/heads/main
git clone "$test_remote" "$test_repo" >/dev/null
trap 'rm -rf "$mock_dir" "$test_dir"' EXIT

base_env=(
  "GITHUB_REPOSITORY=LioRael/lenso"
  "GITHUB_REF=refs/heads/main"
  "GITHUB_EVENT_NAME=workflow_dispatch"
  "GITHUB_TOKEN=test-token"
  "RELEASE_SET=$stage_set"
  "RELEASE_MODE=dry-run"
  "MOCK_MISSING_PACKAGES=lenso-app-plan"
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
  printf '[{"workflow_runs":[{"id":999,"workflow_id":294726715,"name":"CI","path":".github/workflows/ci.yml","event":"push","status":"completed","conclusion":"%s","head_branch":"candidate/test/1","head_sha":"%s","run_attempt":1,"html_url":"https://example.invalid/run/999"}]}]\n' \
    "$run_conclusion" "$sha"
else
  printf 'unexpected gh api request: %s\n' "$args" >&2
  exit 2
fi
EOF
cat >"$mock_dir/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ -n "${MOCK_CURL_STATUS:-}" ]]; then
  printf '%s\n' "$MOCK_CURL_STATUS"
  exit 0
fi
url=''
for arg in "$@"; do url="$arg"; done
path="${url#https://crates.io/api/v1/crates/}"
package="${path%%/*}"
if [[ "$path" == "$package" ]]; then
  if [[ " ${MOCK_UNREGISTERED_PACKAGES:-} " == *" $package "* ]]; then
    printf '404\n'
  else
    printf '200\n'
  fi
elif [[ " ${MOCK_MISSING_PACKAGES:-} " == *" $package "* ]]; then
  printf '404\n'
else
  printf '200\n'
fi
EOF
chmod +x "$mock_dir/gh" "$mock_dir/curl"

expect_failure "empty release stage" "at least one package" \
  run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" RELEASE_SET='[]'

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

mismatched_release_set="$(
  cargo metadata --manifest-path "$ROOT/Cargo.toml" --locked --no-deps --format-version 1 |
    jq -c '[.packages[] | select(.name == "lenso-kernel") | {package_name: .name, version: .version}]'
)"
expect_failure "selected registry version already exists" "not missing from the read-only crates.io plan" \
  run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" RELEASE_SET="$mismatched_release_set" \
  PATH="$mock_dir:$PATH" MOCK_SHA="$current_sha"

agent_version="$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --locked --no-deps --format-version 1 |
  jq -r '.packages[] | select(.name == "lenso-agent-tool-cli-plugin") | .version')"
agent_only="$(jq -cn --arg version "$agent_version" '[{package_name:"lenso-agent-tool-cli-plugin", version:$version}]')"
expect_failure "unpublished dependency omitted from stage" "omits unpublished workspace dependencies" \
  run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" RELEASE_SET="$agent_only" \
  PATH="$mock_dir:$PATH" MOCK_SHA="$current_sha" \
  MOCK_MISSING_PACKAGES='lenso-agent-tool-cli-plugin lenso lenso-app-plan'

expect_failure "new crate cannot use OIDC before owner bootstrap" "has not been first-published" \
  run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" RELEASE_MODE=publish \
  PATH="$mock_dir:$PATH" MOCK_SHA="$current_sha" MOCK_UNREGISTERED_PACKAGES=lenso-app-plan

run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" PATH="$mock_dir:$PATH" \
  MOCK_SHA="$current_sha" MOCK_RUN_CONCLUSION=success MOCK_JOB_CONCLUSION=success >/dev/null
run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" RELEASE_MODE=publish \
  PATH="$mock_dir:$PATH" MOCK_SHA="$current_sha" \
  MOCK_RUN_CONCLUSION=success MOCK_JOB_CONCLUSION=success >/dev/null

expect_failure "registry error is not treated as absence" "unexpected crates.io response 500" \
  run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" PATH="$mock_dir:$PATH" \
  MOCK_SHA="$current_sha" MOCK_CURL_STATUS=500

git -C "$test_repo" push "$test_remote" "$unlanded_sha:refs/heads/main" >/dev/null
expect_failure "publish source is behind main" "must equal the current origin/main SHA" \
  run_gate "${base_env[@]}" RELEASE_SHA="$current_sha" RELEASE_MODE=publish \
  PATH="$mock_dir:$PATH" MOCK_SHA="$unlanded_sha"

env EXPECTED_RELEASE_SET='[]' RELEASE_SHA="$current_sha" bash "$PLAN"
expect_failure "cohort artifact set version mismatch" "source has" \
  env EXPECTED_RELEASE_SET='[{"package_name":"lenso-kernel","version":"0.0.0"}]' \
    RELEASE_SHA="$current_sha" bash "$PLAN"

stage_config_path="$mock_dir/stage.toml"
env RELEASE_SHA="$current_sha" APPROVED_RELEASE_SET="$stage_set" \
  STAGE_CONFIG_PATH="$stage_config_path" bash "$STAGE_CONFIG" >/dev/null
grep -Fxq 'release = false' "$stage_config_path"
grep -Fxq 'name = "lenso-app-plan"' "$stage_config_path"
[[ "$(grep -Fc 'release = true' "$stage_config_path")" == 1 ]]
[[ "$(grep -Fc '[[package]]' "$stage_config_path")" == 1 ]]
if grep -Fq 'name = "lenso-kernel"' "$stage_config_path"; then
  printf '%s\n' 'stage config enabled an out-of-stage package' >&2
  exit 1
fi
engine_version="$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --locked --no-deps --format-version 1 |
  jq -r '.packages[] | select(.name == "lenso-engine") | .version')"
two_package_set="$(jq -cn --arg plan "$app_plan_version" --arg engine "$engine_version" '
  [{package_name:"lenso-app-plan",version:$plan},
   {package_name:"lenso-engine",version:$engine}]')"
env RELEASE_SHA="$current_sha" APPROVED_RELEASE_SET="$two_package_set" \
  STAGE_CONFIG_PATH="$stage_config_path" bash "$STAGE_CONFIG" >/dev/null
enabled_names="$(sed -n 's/^name = "\([^"]*\)"$/\1/p' "$stage_config_path" | sort)"
expected_names="$(jq -r '.[].package_name' <<<"$two_package_set" | sort)"
[[ "$enabled_names" == "$expected_names" ]]
[[ "$(grep -Fc 'release = true' "$stage_config_path")" == 2 ]]
env EXPECTED_RELEASE_SET="$stage_set" \
  ACTUAL_RELEASES="$(jq -cn --arg version "$app_plan_version" \
    '[{package_name:"lenso-app-plan",version:$version,tag:("lenso-app-plan-v"+$version),prs:[]}]')" \
  bash "$VERIFY_STAGE" >/dev/null
expect_failure "published package set drift" "differs from approved stage" \
  env EXPECTED_RELEASE_SET="$stage_set" \
    ACTUAL_RELEASES="$(jq -cn --arg version "$app_plan_version" \
      '[{package_name:"lenso-app-plan",version:$version},
        {package_name:"lenso-kernel",version:"0.3.11"}]')" \
    bash "$VERIFY_STAGE"
expect_failure "stage config rejects wrong source SHA" "does not match RELEASE_SHA" \
  env RELEASE_SHA=0000000000000000000000000000000000000000 \
    APPROVED_RELEASE_SET="$stage_set" STAGE_CONFIG_PATH="$stage_config_path" bash "$STAGE_CONFIG"

printf '%s\n' 'release script negative and cohort-plan tests passed'
