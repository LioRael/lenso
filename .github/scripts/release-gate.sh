#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=release-set.sh
source "$SCRIPT_DIR/release-set.sh"

fail() {
  echo "::error title=Release gate::$*" >&2
  exit 1
}

require_env() {
  [[ -n "${!1:-}" ]] || fail "missing required environment variable: $1"
}

require_env GITHUB_REPOSITORY
require_env GITHUB_REF
require_env GITHUB_EVENT_NAME
require_env RELEASE_SHA
require_env RELEASE_SET
require_env RELEASE_MODE
require_env RELEASE_KIND

[[ "$GITHUB_REPOSITORY" == "LioRael/lenso-protocols" ]] ||
  fail "release workflow is restricted to LioRael/lenso-protocols"
[[ "$GITHUB_REF" == "refs/heads/main" ]] ||
  fail "release workflow must run from refs/heads/main"
[[ "$GITHUB_EVENT_NAME" == "workflow_dispatch" ]] ||
  fail "release workflow requires workflow_dispatch"
case "$RELEASE_MODE" in
  dry-run|publish) ;;
  *) fail "unsupported release mode: $RELEASE_MODE" ;;
esac
case "$RELEASE_KIND" in
  cargo|npm) ;;
  *) fail "unsupported release registry: $RELEASE_KIND" ;;
esac

source_sha="${RELEASE_SHA,,}"
[[ "$source_sha" =~ ^[0-9a-f]{40}$ ]] ||
  fail "source_sha must be a full 40-character hexadecimal commit SHA"
release_set="$(release_set_canonical "$RELEASE_KIND" "$RELEASE_SET")" ||
  fail "release_set is invalid for $RELEASE_KIND"

git fetch origin main --no-tags >/dev/null
main_sha="$(git rev-parse refs/remotes/origin/main)" ||
  fail "origin/main is unavailable after fetch"
[[ "$(git rev-parse HEAD)" == "$source_sha" ]] ||
  fail "checked-out source does not match source_sha"
git cat-file -e "$source_sha^{commit}" ||
  fail "source_sha is not a commit available to the checkout"
git merge-base --is-ancestor "$source_sha" "$main_sha" ||
  fail "source_sha is not reachable from the current origin/main"

case "$RELEASE_KIND" in
  cargo)
    metadata="$(cargo metadata --locked --no-deps --format-version 1)" ||
      fail "cargo metadata failed for source_sha"
    while IFS=$'\t' read -r package expected_version; do
      package_record="$(
        jq -c --arg package "$package" \
          '[.packages[] | select(.name == $package)] | if length == 1 then .[0] else empty end' \
          <<<"$metadata"
      )"
      [[ -n "$package_record" ]] ||
        fail "release_set names a package outside this workspace: $package"
      [[ "$(jq -c '.publish' <<<"$package_record")" != "[]" ]] ||
        fail "release_set names a publish=false package: $package"
      actual_version="$(jq -r '.version' <<<"$package_record")"
      [[ "$actual_version" == "$expected_version" ]] ||
        fail "release_set version for $package is $expected_version, source has $actual_version"
    done < <(jq -r '.[] | [.package_name, .version] | @tsv' <<<"$release_set")
    ;;
  npm)
    while IFS=$'\t' read -r package expected_version; do
      case "$package" in
        @lenso/contract-runtime)
          manifest="packages/lenso-contract-runtime/package.json"
          ;;
        @lenso/process-protocol)
          manifest="packages/lenso-process-protocol/package.json"
          ;;
        *)
          fail "release_set names an npm package outside the publish allowlist: $package"
          ;;
      esac
      actual_version="$(node -p "require('./$manifest').version")"
      [[ "$actual_version" == "$expected_version" ]] ||
        fail "release_set version for $package is $expected_version, source has $actual_version"
    done < <(jq -r '.[] | [.package_name, .version] | @tsv' <<<"$release_set")
    ;;
esac

printf 'Release gate passed for %s (%s): %s\n' "$source_sha" "$RELEASE_KIND" "$release_set"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    printf 'source_sha=%s\n' "$source_sha"
    printf 'release_set=%s\n' "$release_set"
  } >>"$GITHUB_OUTPUT"
fi
