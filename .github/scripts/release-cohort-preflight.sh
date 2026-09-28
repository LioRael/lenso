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
source_dirs=()
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
  source_dir="$(cd -- "$(dirname -- "$manifest_path")" && pwd -P)"
  case "$source_dir" in
    "$source_root"/*) ;;
    *) fail "package source is outside the exact release snapshot: $package" ;;
  esac
  packages+=("$package")
  versions+=("$actual_version")
  source_dirs+=("$source_dir")
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

private_fixture_edges="$(jq -r '
    .packages as $workspace
    | .packages[]
    | .name as $owner
    | .dependencies[]?
    | select(.kind == "dev" and .source == null and (.path // "") != "" and .req == "*")
    | .name as $fixture
    | select(any($workspace[]; .name == $fixture and .publish == []))
    | [$owner, $fixture] | @tsv
  ' <<<"$metadata")" || fail "could not identify private path-only dev fixture edges"

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

prefetch_dependencies() {
  local package="$1"
  jq -r --arg package "$package" '
    [.packages[] | select(.name == $package)]
    | if length == 1 then .[0] else error("release package must occur once in metadata") end
    | .dependencies[]?
    | select((.kind != "dev" or .req != "*") and .source == null and (.path // "") != "")
    | .name
  ' <<<"$metadata"
}

published_transitive_workspace_dependencies() {
  local workspace_metadata="$1"
  local published_metadata="$2"
  jq -r --slurpfile workspace <(printf '%s\n' "$workspace_metadata") '
    .packages[]
    | select(.source == "registry+https://github.com/rust-lang/crates.io-index")
    | .name as $name | .version as $version
    | select(any($workspace[0].packages[]; .name == $name and .version == $version))
    | [$name, $version] | @tsv
  ' <<<"$published_metadata"
}

published_workspace_requirements() {
  local workspace_metadata="$1"
  local published_metadata="$2"
  jq -r --slurpfile workspace <(printf '%s\n' "$workspace_metadata") '
    .packages[]
    | select(.source == "registry+https://github.com/rust-lang/crates.io-index")
    | .dependencies[]?
    | select(.source == "registry+https://github.com/rust-lang/crates.io-index")
    | .name as $name
    | [$workspace[0].packages[] | select(.name == $name)] as $matches
    | if ($matches | length) == 0 then empty
      elif ($matches | length) == 1 then $matches[0] | [.name, .version] | @tsv
      else error("published requirement matches multiple workspace packages: " + $name)
      end
  ' <<<"$published_metadata" | sort -u
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

registry_dependencies=()
registry_versions=()
registry_source_dirs=()
for package in "${packages[@]}"; do
  while IFS= read -r dependency; do
    [[ -z "$dependency" ]] && continue
    package_index "$dependency" >/dev/null && continue
    contains "$dependency" "${registry_dependencies[@]-}" && continue

    dependency_record="$(jq -ce --arg dependency "$dependency" '
        [.packages[] | select(.name == $dependency)]
        | if length == 1 then .[0] else error("path dependency must occur once in workspace metadata") end
      ' <<<"$metadata")" || fail "out-of-cohort path dependency is not a workspace package: $dependency"
    dependency_manifest="$(jq -r '.manifest_path' <<<"$dependency_record")"
    grep -Eq '^[[:space:]]*publish[[:space:]]*=[[:space:]]*true[[:space:]]*$' "$dependency_manifest" ||
      fail "out-of-cohort workspace dependency is not publish=true: $dependency"
    dependency_dir="$(cd -- "$(dirname -- "$dependency_manifest")" && pwd -P)"
    case "$dependency_dir" in
      "$source_root"/*) ;;
      *) fail "out-of-cohort workspace dependency is outside the exact release snapshot: $dependency" ;;
    esac
    registry_dependencies+=("$dependency")
    registry_versions+=("$(jq -r '.version' <<<"$dependency_record")")
    registry_source_dirs+=("$dependency_dir")
  done < <(prefetch_dependencies "$package")
done

if (( ${#registry_dependencies[@]} > 0 )); then
  prefetch_root="$scratch/registry-dependencies"
  mkdir -p "$prefetch_root/src"
  touch "$prefetch_root/src/lib.rs"
  {
    printf '%s\n' '[package]' 'name = "lenso-cohort-registry-prefetch"' 'version = "0.0.0"' 'edition = "2024"' '' '[dependencies]'
    for index in "${!registry_dependencies[@]}"; do
      printf 'Fetching exact registry package %s@%s\n' "${registry_dependencies[$index]}" "${registry_versions[$index]}" >&2
      printf '%s = "=%s"\n' "${registry_dependencies[$index]}" "${registry_versions[$index]}"
    done
  } >"$prefetch_root/Cargo.toml"
  cargo fetch --manifest-path "$prefetch_root/Cargo.toml" ||
    fail "could not fetch exact published out-of-cohort workspace dependencies"
  prefetch_metadata="$(cargo metadata --locked --offline --manifest-path "$prefetch_root/Cargo.toml" --format-version 1)" ||
    fail "could not inspect fetched registry dependencies"

  # Cargo can omit a registry dependency's dev or optional dependencies until
  # its published source is overlaid into this workspace. Fetch the matching
  # workspace identity for each named dependency before the offline locked
  # check; Cargo then validates the published version requirement and lock.
  prefetch_requested=("${registry_dependencies[@]}")
  while :; do
    added=false
    published_requirements="$(published_workspace_requirements "$metadata" "$prefetch_metadata")" ||
      fail "could not inspect published workspace requirements"
    while IFS=$'\t' read -r dependency version; do
      [[ -n "$dependency" ]] || continue
      contains "$dependency" "${prefetch_requested[@]}" && continue
      package_index "$dependency" >/dev/null &&
        fail "published dependency requires an unshipped cohort package: $dependency@$version"
      dependency_record="$(jq -ce --arg dependency "$dependency" --arg version "$version" '
          [.packages[] | select(.name == $dependency and .version == $version)]
          | if length == 1 then .[0] else error("exact published requirement must match one workspace package") end
        ' <<<"$metadata")" || fail "could not locate exact published requirement: $dependency@$version"
      dependency_manifest="$(jq -r '.manifest_path' <<<"$dependency_record")"
      grep -Eq '^[[:space:]]*publish[[:space:]]*=[[:space:]]*true[[:space:]]*$' "$dependency_manifest" ||
        fail "published dependency requires a private workspace package: $dependency@$version"
      printf 'Fetching exact published requirement %s@%s\n' "$dependency" "$version" >&2
      printf '%s = "=%s"\n' "$dependency" "$version" >>"$prefetch_root/Cargo.toml"
      prefetch_requested+=("$dependency")
      added=true
    done <<<"$published_requirements"
    [[ "$added" == true ]] || break
    cargo fetch --manifest-path "$prefetch_root/Cargo.toml" ||
      fail "could not fetch exact published workspace requirements"
    prefetch_metadata="$(cargo metadata --locked --offline --manifest-path "$prefetch_root/Cargo.toml" --format-version 1)" ||
      fail "could not inspect exact published workspace requirements"
  done

  # A published direct dependency can refer to another crate in this workspace
  # using only its registry identity. Stage that exact published source too, so
  # the scratch lock does not gain a second identity for the same local crate.
  while IFS=$'\t' read -r dependency version; do
    [[ -n "$dependency" ]] || continue
    package_index "$dependency" >/dev/null && continue
    contains "$dependency" "${registry_dependencies[@]-}" && continue
    dependency_record="$(jq -ce --arg dependency "$dependency" --arg version "$version" '
        [.packages[] | select(.name == $dependency and .version == $version)]
        | if length == 1 then .[0] else error("published transitive dependency must match one workspace package") end
      ' <<<"$metadata")" || fail "could not locate transitive workspace dependency: $dependency@$version"
    dependency_manifest="$(jq -r '.manifest_path' <<<"$dependency_record")"
    grep -Eq '^[[:space:]]*publish[[:space:]]*=[[:space:]]*true[[:space:]]*$' "$dependency_manifest" ||
      fail "transitive workspace dependency is not publish=true: $dependency@$version"
    dependency_dir="$(cd -- "$(dirname -- "$dependency_manifest")" && pwd -P)"
    case "$dependency_dir" in
      "$source_root"/*) ;;
      *) fail "transitive workspace dependency is outside the exact release snapshot: $dependency@$version" ;;
    esac
    registry_dependencies+=("$dependency")
    registry_versions+=("$version")
    registry_source_dirs+=("$dependency_dir")
  done < <(published_transitive_workspace_dependencies "$metadata" "$prefetch_metadata")

  mkdir -p "$scratch/published-sources" "$scratch/retired-registry-sources"
  for index in "${!registry_dependencies[@]}"; do
    dependency="${registry_dependencies[$index]}"
    version="${registry_versions[$index]}"
    registry_record="$(jq -ce --arg dependency "$dependency" --arg version "$version" '
        [.packages[] | select(.name == $dependency and .version == $version and .source == "registry+https://github.com/rust-lang/crates.io-index")]
        | if length == 1 then .[0] else error("exact crates.io package must occur once in prefetch metadata") end
      ' <<<"$prefetch_metadata")" || fail "could not locate exact fetched registry source: $dependency@$version"
    registry_manifest="$(jq -r '.manifest_path' <<<"$registry_record")"
    [[ -f "$registry_manifest" ]] || fail "fetched registry source has no manifest: $dependency@$version"
    published_dir="$scratch/published-sources/$dependency-$version"
    cp -R -- "$(dirname -- "$registry_manifest")" "$published_dir" ||
      fail "could not stage exact fetched registry source: $dependency@$version"
    mv -- "${registry_source_dirs[$index]}" "$scratch/retired-registry-sources/$dependency-$version" ||
      fail "could not retire workspace source for published dependency: $dependency@$version"
    ln -s -- "$published_dir" "${registry_source_dirs[$index]}" ||
      fail "could not overlay exact fetched registry source: $dependency@$version"
    printf 'Staged exact registry source %s@%s\n' "$dependency" "$version"
  done
fi

completed_packages=()
completed_dirs=()

build_completed_patch_args() {
  patch_args=()
  local index
  for index in "${!registry_dependencies[@]}"; do
    patch_args+=(
      --config
      "patch.crates-io.${registry_dependencies[$index]}.path=\"${registry_source_dirs[$index]}\""
    )
  done
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

verify_fixture_only_lock_change() {
  local before="$1"
  local after="$2"
  local allowed="$3"
  local expected="$scratch/expected-normalized-Cargo.lock"
  awk -F '\t' '
    FNR == NR { removable[$1 SUBSEP $2] = 1; next }
    /^\[\[package\]\]$/ { owner = ""; in_dependencies = 0 }
    /^name = "/ && owner == "" {
      owner = $0
      sub(/^name = "/, "", owner)
      sub(/"$/, "", owner)
    }
    /^dependencies = \[$/ { in_dependencies = 1 }
    in_dependencies && /^ "[^"]+",$/ {
      dependency = $0
      sub(/^ "/, "", dependency)
      sub(/",$/, "", dependency)
      if (removable[owner SUBSEP dependency]) next
    }
    in_dependencies && /^\]$/ { in_dependencies = 0 }
    { print }
  ' "$allowed" "$before" >"$expected"
  ! cmp -s "$before" "$after" && cmp -s "$expected" "$after"
}

validate_or_normalize_scratch_lock() {
  local before="$scratch/before-normalization-Cargo.lock"
  local allowed="$scratch/allowed-private-fixture-edges.tsv"
  local index owner edge_owner fixture artifact_manifest

  if (cd "$source_root" && run_cargo_with_completed_patches metadata --locked --offline --format-version 1 >/dev/null) 2>"$scratch/locked-metadata.err"; then
    return 0
  fi

  : >"$allowed"
  for index in "${!completed_packages[@]}"; do
    owner="${completed_packages[$index]}"
    artifact_manifest="${completed_dirs[$index]}/Cargo.toml"
    while IFS=$'\t' read -r edge_owner fixture; do
      [[ "$edge_owner" == "$owner" && -n "$fixture" ]] || continue
      grep -Fq "$fixture" "$artifact_manifest" && continue
      printf '%s\t%s\n' "$owner" "$fixture" >>"$allowed"
    done <<<"$private_fixture_edges"
  done
  if [[ ! -s "$allowed" ]]; then
    sed -n '1,30p' "$scratch/locked-metadata.err" >&2
    fail "locked metadata failed without an omitted private dev fixture"
  fi

  cp "$source_root/Cargo.lock" "$before"
  (cd "$source_root" && run_cargo_with_completed_patches metadata --offline --format-version 1 >/dev/null) ||
    fail "could not inspect scratch-only lock normalization"
  if ! verify_fixture_only_lock_change "$before" "$source_root/Cargo.lock" "$allowed"; then
    diff -u "$before" "$source_root/Cargo.lock" >&2 || true
    fail "scratch lock drift exceeds omitted private path-only dev fixture edges"
  fi
  (cd "$source_root" && run_cargo_with_completed_patches metadata --locked --offline --format-version 1 >/dev/null) ||
    fail "locked metadata still fails after guarded scratch-only normalization"
  printf 'Normalized scratch lock only for omitted private dev fixture edges\n'
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
    validate_or_normalize_scratch_lock
    (
      cd "$source_root"
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

    # Keep path dependencies pointing at this package, but make the packaged
    # artifact its sole identity in the source workspace and registry patch.
    source_dir="${source_dirs[$index]}"
    mkdir -p "$scratch/packed-sources"
    mv -- "$source_dir" "$scratch/packed-sources/$package-$version" ||
      fail "could not retire the packed source for $package"
    ln -s -- "$extracted_dir" "$source_dir" ||
      fail "could not link the packed artifact for $package"

    completed_packages+=("$package")
    completed_dirs+=("$source_dir")
    artifact_records="$(jq -c --arg package "$package" --arg version "$version" --arg digest "$digest" \
      '. + [{package_name: $package, version: $version, sha256: $digest}]' <<<"$artifact_records")"
    made_progress=true
  done
  [[ "$made_progress" == true ]] ||
    fail "could not derive a topological order for the exact release cohort"
done

build_completed_patch_args
validate_or_normalize_scratch_lock

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
