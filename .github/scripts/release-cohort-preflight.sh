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

contains() {
  local needle="$1"
  shift
  local value
  for value in "$@"; do
    [[ "$value" == "$needle" ]] && return 0
  done
  return 1
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
[[ "$(git -C "$ROOT" rev-parse HEAD)" == "$release_sha" ]] ||
  fail "checked-out source does not match RELEASE_SHA"
git -C "$ROOT" cat-file -e "$release_sha^{commit}" ||
  fail "RELEASE_SHA is not an available commit"

scratch="$(mktemp -d "${TMPDIR:-/tmp}/lenso-release-cohort.XXXXXX")" ||
  fail "could not create a temporary cohort workspace"
scratch="$(cd -- "$scratch" && pwd -P)"
cleanup() {
  rm -rf -- "$scratch"
}
trap cleanup EXIT

source_root="$scratch/source"
artifact_root="$scratch/artifacts"
package_target="$scratch/package-target"
clean_room="$scratch/clean-room"
mkdir -p "$source_root" "$artifact_root" "$package_target" "$clean_room/packages"
git -C "$ROOT" archive --format=tar "$release_sha" | tar -x -C "$source_root"
source_root="$(cd -- "$source_root" && pwd -P)"

[[ -f "$source_root/Cargo.lock" ]] ||
  fail "the release source does not contain Cargo.lock"

metadata="$(cd "$source_root" && cargo metadata --locked --no-deps --format-version 1)" ||
  fail "cargo metadata failed for the release source"

packages=()
versions=()
artifact_records='[]'
while IFS=$'\t' read -r package expected_version; do
  package_record="$(jq -ce --arg package "$package" '
      [.packages[] | select(.name == $package)]
      | if length == 1 then .[0] else error("release package must occur once in metadata") end
    ' <<<"$metadata")" || fail "release set names a package outside this workspace: $package"
  manifest_path="$(jq -r '.manifest_path' <<<"$package_record")"
  actual_version="$(jq -r '.version' <<<"$package_record")"
  [[ "$actual_version" == "$expected_version" ]] ||
    fail "release set version for $package is $expected_version, source has $actual_version"
  grep -Eq '^[[:space:]]*publish[[:space:]]*=[[:space:]]*true[[:space:]]*$' "$manifest_path" ||
    fail "package is not in the publish=true allowlist: $package"
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
      printf -- '- Package order was derived from exact workspace dependencies.\n'
      printf -- '- No package upload, tag, or GitHub release operation was performed.\n'
      printf -- '- The source overlay models only already-packed cohort predecessors; it does not claim registry propagation evidence.\n'
      jq -r '.[] | "- `\(.package_name)@\(.version)`: SHA-256 `\(.sha256)`"' <<<"$artifact_records"
    } >>"$GITHUB_STEP_SUMMARY"
  fi
}

if (( ${#packages[@]} == 0 )); then
  report_success
  exit 0
fi

(cd "$source_root" && cargo fetch --locked) ||
  fail "could not fetch the locked non-cohort dependencies"

source_dependencies() {
  local package="$1"
  jq -r --arg package "$package" '
    [.packages[] | select(.name == $package)]
    | if length == 1 then .[0] else error("release package must occur once in metadata") end
    | .dependencies[]?
    | select(.source == null and (.path // "") != "")
    | .name
  ' <<<"$metadata"
}

package_index() {
  local package="$1"
  local index
  for index in "${!packages[@]}"; do
    [[ "${packages[$index]}" == "$package" ]] && {
      printf '%s\n' "$index"
      return 0
    }
  done
  return 1
}

completed_packages=()
completed_dirs=()

build_completed_patch_args() {
  patch_args=()
  local index
  for index in "${!completed_packages[@]}"; do
    patch_args+=(
      --config
      "patch.crates-io.${completed_packages[$index]}.path=\"${completed_dirs[$index]}\""
    )
  done
}

run_cargo_with_completed_patches() {
  if (( ${#patch_args[@]} == 0 )); then
    cargo "$@"
  else
    cargo "${patch_args[@]}" "$@"
  fi
}

while (( ${#completed_packages[@]} < ${#packages[@]} )); do
  made_progress=false
  for index in "${!packages[@]}"; do
    package="${packages[$index]}"
    contains "$package" "${completed_packages[@]-}" && continue

    waiting_on=()
    while IFS= read -r dependency; do
      [[ -z "$dependency" ]] && continue
      if package_index "$dependency" >/dev/null &&
        ! contains "$dependency" "${completed_packages[@]-}"; then
        waiting_on+=("$dependency")
      fi
    done < <(source_dependencies "$package")
    (( ${#waiting_on[@]} == 0 )) || continue

    build_completed_patch_args
    (
      cd "$source_root"
      run_cargo_with_completed_patches metadata --offline --format-version 1 >/dev/null
      run_cargo_with_completed_patches package --locked --offline --no-verify \
        --target-dir "$package_target" -p "$package"
    ) || fail "could not package $package from the exact cohort source"

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

    completed_packages+=("$package")
    completed_dirs+=("$(cd -- "$extracted_dir" && pwd -P)")
    artifact_records="$(jq -c --arg package "$package" --arg version "$version" --arg digest "$digest" \
      '. + [{package_name: $package, version: $version, sha256: $digest}]' <<<"$artifact_records")"
    made_progress=true
  done
  [[ "$made_progress" == true ]] ||
    fail "could not derive a topological order for the exact release cohort"
done

actual_release_set="$(jq -c '[.[] | {package_name, version}] | sort_by(.package_name)' <<<"$artifact_records")"
[[ "$actual_release_set" == "$expected" ]] ||
  fail "artifact set does not match the approved release set"

for index in "${!packages[@]}"; do
  package="${packages[$index]}"
  version="${versions[$index]}"
  tar -xzf "$package_target/package/$package-$version.crate" -C "$clean_room/packages" ||
    fail "could not stage the clean-room artifact for $package"
done
{
  printf '%s\n' '[workspace]' 'members = ['
  for index in "${!packages[@]}"; do
    printf '  "packages/%s-%s",\n' "${packages[$index]}" "${versions[$index]}"
  done
  printf '%s\n' ']' 'resolver = "3"'
} >"$clean_room/Cargo.toml"
cp "$source_root/Cargo.lock" "$clean_room/Cargo.lock"
clean_room="$(cd -- "$clean_room" && pwd -P)"

clean_room_patch_args=()
for index in "${!packages[@]}"; do
  clean_room_patch_args+=(
    --config
    "patch.crates-io.${packages[$index]}.path=\"$clean_room/packages/${packages[$index]}-${versions[$index]}\""
  )
done
(
  cd "$clean_room"
  cargo "${clean_room_patch_args[@]}" metadata --offline --format-version 1 >/dev/null
  cargo "${clean_room_patch_args[@]}" check --workspace --locked --all-targets
  cargo "${clean_room_patch_args[@]}" test --workspace --locked --no-run
) || fail "the extracted cohort artifacts did not compile in the clean-room workspace"

report_success
