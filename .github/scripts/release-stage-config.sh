#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
# shellcheck source=release-set.sh
source "$SCRIPT_DIR/release-set.sh"

fail() {
  echo "::error title=Release stage config::$*" >&2
  exit 1
}

[[ -n "${RELEASE_SHA:-}" && -n "${APPROVED_RELEASE_SET:-}" && -n "${STAGE_CONFIG_PATH:-}" ]] ||
  fail "RELEASE_SHA, APPROVED_RELEASE_SET, and STAGE_CONFIG_PATH are required"
[[ "$STAGE_CONFIG_PATH" == /* ]] || fail "STAGE_CONFIG_PATH must be absolute"
[[ "$(git -C "$ROOT" rev-parse HEAD)" == "${RELEASE_SHA,,}" ]] ||
  fail "checked-out source does not match RELEASE_SHA"

approved="$(release_set_canonical "$APPROVED_RELEASE_SET")" ||
  fail "approved release set is invalid"
[[ "$approved" != '[]' ]] || fail "approved release set is empty"

metadata="$(cd "$ROOT" && cargo metadata --locked --no-deps --format-version 1)" ||
  fail "cargo metadata failed for the release source"
while IFS=$'\t' read -r package version; do
  record="$(jq -ce --arg name "$package" '
    [.packages[] | select(.name == $name)]
    | if length == 1 then .[0] else error("package must occur once") end
  ' <<<"$metadata")" || fail "release package is outside the workspace: $package"
  [[ "$(jq -r '.version' <<<"$record")" == "$version" ]] ||
    fail "release version differs from source: $package@$version"
  manifest="$(jq -r '.manifest_path' <<<"$record")"
  grep -Eq '^[[:space:]]*publish[[:space:]]*=[[:space:]]*true[[:space:]]*$' "$manifest" ||
    fail "package is not in the publish=true allowlist: $package"
done < <(jq -r '.[] | [.package_name, .version] | @tsv' <<<"$approved")

base="$ROOT/.github/release-plz.toml"
grep -Eq '^[[:space:]]*release[[:space:]]*=[[:space:]]*false[[:space:]]*$' "$base" ||
  fail "base release-plz config must disable workspace release"
if grep -Eq '^[[:space:]]*\[\[package\]\]' "$base" ||
  grep -Eq '^[[:space:]]*release[[:space:]]*=[[:space:]]*true[[:space:]]*$' "$base"; then
  fail "base release-plz config must not enable package releases"
fi
config_dir="$(dirname -- "$STAGE_CONFIG_PATH")"
[[ -d "$config_dir" ]] || fail "stage config directory does not exist: $config_dir"
umask 077
staged="$(mktemp "$STAGE_CONFIG_PATH.XXXXXX")" || fail "could not create stage config"
trap 'rm -f -- "$staged"' EXIT
cp -- "$base" "$staged"
while IFS= read -r package; do
  printf '\n[[package]]\nname = "%s"\nrelease = true\n' "$package" >>"$staged"
done < <(jq -r '.[].package_name' <<<"$approved")
mv -- "$staged" "$STAGE_CONFIG_PATH"
trap - EXIT
printf 'Generated release-plz stage config for %s\n' "$approved"
