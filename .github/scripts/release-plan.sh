#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=release-set.sh
source "$SCRIPT_DIR/release-set.sh"

fail() {
  echo "::error title=Release plan::$*" >&2
  exit 1
}

: "${EXPECTED_RELEASE_SET:?EXPECTED_RELEASE_SET is required}"
: "${ACTUAL_RELEASES:?ACTUAL_RELEASES is required}"

expected="$(release_set_canonical cargo "$EXPECTED_RELEASE_SET")" ||
  fail "expected release set is invalid"
actual="$(release_releases_canonical "$ACTUAL_RELEASES")" ||
  fail "release-plz dry-run output is invalid"
[[ "$actual" == "$expected" ]] ||
  fail "release-plz dry-run emitted an unexpected release set: expected $expected, got $actual"

printf 'Release-plz dry-run exactly matched: %s\n' "$actual"
