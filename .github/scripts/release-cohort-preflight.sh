#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
# shellcheck source=release-set.sh
source "$SCRIPT_DIR/release-set.sh"

fail() {
  echo "::error title=Cohort artifact preflight::$*" >&2
  exit 1
}

require_env() {
  [[ -n "${!1:-}" ]] || fail "missing required environment variable: $1"
}

sha256_file() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    fail "neither sha256sum nor shasum is available"
  fi
}

require_env EXPECTED_RELEASE_SET
require_env RELEASE_SHA

expected="$(release_set_canonical "$EXPECTED_RELEASE_SET")" ||
  fail "expected release set is invalid"
release_sha="$(printf '%s' "$RELEASE_SHA" | tr '[:upper:]' '[:lower:]')"
[[ "$release_sha" =~ ^[0-9a-f]{40}$ ]] ||
  fail "RELEASE_SHA must be a full 40-character hexadecimal commit SHA"
git -C "$ROOT" cat-file -e "$release_sha^{commit}" ||
  fail "RELEASE_SHA is not an available commit"

scratch="$(mktemp -d "${TMPDIR:-/tmp}/lenso-runtime-release-cohort.XXXXXX")" ||
  fail "could not create a temporary cohort workspace"
scratch="$(cd -- "$scratch" && pwd -P)"
cleanup() {
  rm -rf -- "$scratch"
}
trap cleanup EXIT

source_root="$scratch/source"
artifact_root="$scratch/clean-room/packages"
package_target="$scratch/package-target"
clean_room="$scratch/clean-room"
mkdir -p "$source_root" "$artifact_root" "$package_target"
git -C "$ROOT" archive --format=tar "$release_sha" | tar -x -C "$source_root"
source_root="$(cd -- "$source_root" && pwd -P)"

[[ -f "$source_root/Cargo.lock" ]] ||
  fail "the release source does not contain Cargo.lock"
metadata="$(cd "$source_root" && cargo metadata --locked --no-deps --format-version 1)" ||
  fail "cargo metadata failed for the release source"
publishable_filter='(.publish == null or ((.publish | type) == "array" and (.publish | index("crates-io") != null)))'

packages=()
versions=()
artifact_records='[]'
while IFS=$'\t' read -r package expected_version; do
  package_record="$(jq -ce --arg package "$package" '
      [.packages[] | select(.name == $package)]
      | if length == 1 then .[0] else error("release package must occur once in metadata") end
    ' <<<"$metadata")" || fail "release set names a package outside this workspace: $package"
  actual_version="$(jq -r '.version' <<<"$package_record")"
  [[ "$actual_version" == "$expected_version" ]] ||
    fail "release set version for $package is $expected_version, source has $actual_version"
  jq -e "$publishable_filter" <<<"$package_record" >/dev/null ||
    fail "package is not publishable to crates.io: $package"
  packages+=("$package")
  versions+=("$actual_version")
done < <(jq -r '.[] | [.package_name, .version] | @tsv' <<<"$expected")

report_success() {
  printf 'Cohort artifact preflight completed: %s\n' "$artifact_records"
  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    printf 'cohort_artifacts=%s\n' "$artifact_records" >>"$GITHUB_OUTPUT"
  fi
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    {
      printf '### Exact cohort artifact preflight\n\n'
      printf -- '- Source: `%s`\n' "$release_sha"
      printf -- '- The source lock was validated before packaging.\n'
      printf -- '- Cargo prepared the full cohort for crates.io together, so interdependent package locks model their registry identities.\n'
      printf -- '- Extracted artifacts compiled through an external clean-room consumer.\n'
      printf -- '- No package upload, tag, or GitHub release operation was performed.\n'
      printf -- '- Producer-only dev targets remain covered by source CI; the consumer proof does not require unpublished test harnesses.\n'
      jq -r '.[] | "- `\(.package_name)@\(.version)`: SHA-256 `\(.sha256)`"' <<<"$artifact_records"
    } >>"$GITHUB_STEP_SUMMARY"
  fi
}

if (( ${#packages[@]} == 0 )); then
  report_success
  exit 0
fi

(cd "$source_root" && cargo fetch --locked) ||
  fail "could not fetch the locked source dependencies"

package_args=()
for package in "${packages[@]}"; do
  package_args+=(-p "$package")
done
(
  cd "$source_root"
  cargo package --no-verify --registry crates-io --target-dir "$package_target" "${package_args[@]}"
) || fail "could not package the exact cohort for crates.io"

for index in "${!packages[@]}"; do
  package="${packages[$index]}"
  version="${versions[$index]}"
  artifact="$package_target/package/$package-$version.crate"
  [[ -f "$artifact" ]] ||
    fail "cargo package did not produce $package-$version.crate"
  tar -xzf "$artifact" -C "$artifact_root" ||
    fail "could not extract the artifact for $package"
  extracted_dir="$artifact_root/$package-$version"
  [[ -f "$extracted_dir/Cargo.toml" ]] ||
    fail "the artifact for $package has no packaged Cargo.toml"
  digest="$(sha256_file "$artifact")"
  [[ "$digest" =~ ^[0-9a-f]{64}$ ]] ||
    fail "could not calculate the SHA-256 digest for $package"
  artifact_records="$(jq -c --arg package "$package" --arg version "$version" --arg digest "$digest" \
    '. + [{package_name: $package, version: $version, sha256: $digest}]' <<<"$artifact_records")"
done

actual_release_set="$(jq -c '[.[] | {package_name, version}] | sort_by(.package_name)' <<<"$artifact_records")"
[[ "$actual_release_set" == "$expected" ]] ||
  fail "artifact set does not match the approved release set"

consumer="$clean_room/consumer"
mkdir -p "$consumer/src"
{
  printf '%s\n' '[package]' 'name = "lenso-release-cohort-consumer"' 'version = "0.0.0"' 'edition = "2024"' '' '[dependencies]'
  for index in "${!packages[@]}"; do
    printf '%s = { version = "=%s" }\n' "${packages[$index]}" "${versions[$index]}"
  done
  printf '%s\n' '' '[patch.crates-io]'
  for index in "${!packages[@]}"; do
    printf '%s = { path = "../packages/%s-%s" }\n' \
      "${packages[$index]}" "${packages[$index]}" "${versions[$index]}"
  done
} >"$consumer/Cargo.toml"
printf '%s\n' 'pub fn release_cohort_consumer() {}' >"$consumer/src/lib.rs"
(
  cd "$consumer"
  cargo generate-lockfile
  cargo check --locked
  cargo test --locked --no-run
) || fail "the extracted cohort artifacts did not compile for a clean-room consumer"

report_success
