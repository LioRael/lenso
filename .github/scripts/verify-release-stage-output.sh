#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# shellcheck source=release-set.sh
source "$SCRIPT_DIR/release-set.sh"

[[ -n "${EXPECTED_RELEASE_SET:-}" && -n "${ACTUAL_RELEASES:-}" ]] || {
  echo "::error::EXPECTED_RELEASE_SET and ACTUAL_RELEASES are required" >&2
  exit 1
}
expected="$(release_set_canonical "$EXPECTED_RELEASE_SET")" || exit 1
actual="$(release_releases_canonical "$ACTUAL_RELEASES")" || exit 1
[[ "$expected" != '[]' && "$actual" == "$expected" ]] || {
  echo "::error::release-plz package set differs from approved stage: expected ${expected}, actual ${actual}" >&2
  exit 1
}
printf 'Release-plz package set matches approved stage: %s\n' "$actual"
