#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=release-set.sh
source "$SCRIPT_DIR/release-set.sh"

fail() {
  echo "::error title=Release publish::$*" >&2
  exit 1
}

: "${EXPECTED_RELEASE_SET:?EXPECTED_RELEASE_SET is required}"
: "${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN is required}"

expected="$(release_set_canonical cargo "$EXPECTED_RELEASE_SET")" ||
  fail "expected release set is invalid"

while IFS=$'\t' read -r package expected_version; do
  actual_version="$(cargo metadata --locked --no-deps --format-version 1 \
    | jq -r --arg package "$package" '.packages[] | select(.name == $package) | .version')"
  [[ "$actual_version" == "$expected_version" ]] ||
    fail "release set version for $package is $expected_version, source has $actual_version"
  cargo publish --locked -p "$package"
done < <(jq -r '.[] | [.package_name, .version] | @tsv' <<<"$expected")

printf 'Exact cargo publish completed: %s\n' "$expected"
