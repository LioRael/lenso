#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
# shellcheck source=release-set.sh
source "$SCRIPT_DIR/release-set.sh"

fail() {
  echo "::error title=Cohort publish::$*" >&2
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

# Cargo obtains the crates.io publishing credential through GitHub Actions OIDC
# when this job has id-token: write. Do not configure a long-lived token here.
registry_checksum() {
  local package="$1"
  local version="$2"
  local payload status body checksum
  payload="$(curl --silent --show-error --location --retry 2 \
    --user-agent 'Lenso-runtime-cohort-publish/1.0' \
    --write-out $'\n%{http_code}' \
    "https://crates.io/api/v1/crates/${package}/${version}")" ||
    fail "could not query crates.io for ${package}@${version}"
  status="${payload##*$'\n'}"
  body="${payload%$'\n'*}"
  case "$status" in
    404) return 1 ;;
    200)
      checksum="$(jq -er '.version.checksum' <<<"$body")" ||
        fail "crates.io did not return a checksum for ${package}@${version}"
      [[ "$checksum" =~ ^[0-9a-f]{64}$ ]] ||
        fail "crates.io returned an invalid checksum for ${package}@${version}"
      printf '%s\n' "$checksum"
      ;;
    *) fail "unexpected crates.io response ${status} for ${package}@${version}" ;;
  esac
}

wait_for_registry_checksum() {
  local package="$1"
  local version="$2"
  local expected_checksum="$3"
  local attempts="${REGISTRY_VISIBILITY_ATTEMPTS:-60}"
  local delay="${REGISTRY_VISIBILITY_DELAY_SECONDS:-5}"
  [[ "$attempts" =~ ^[1-9][0-9]*$ ]] || fail "REGISTRY_VISIBILITY_ATTEMPTS must be positive"
  [[ "$delay" =~ ^[0-9]+$ ]] || fail "REGISTRY_VISIBILITY_DELAY_SECONDS must be non-negative"

  local attempt observed
  for ((attempt = 1; attempt <= attempts; attempt++)); do
    if observed="$(registry_checksum "$package" "$version")"; then
      [[ "$observed" == "$expected_checksum" ]] ||
        fail "crates.io already has a different artifact for ${package}@${version}"
      printf 'Registry visibility confirmed for %s@%s after attempt %s\n' "$package" "$version" "$attempt"
      return 0
    fi
    (( attempt == attempts )) && break
    sleep "$delay"
  done
  fail "timed out waiting for crates.io visibility of ${package}@${version}"
}

require_env EXPECTED_RELEASE_SET
require_env RELEASE_SHA
require_env RELEASE_MODE
[[ "$RELEASE_MODE" == "publish" ]] ||
  fail "direct cohort publishing requires RELEASE_MODE=publish"
[[ "${RELEASE_CONFIRMATION:-}" == "publish" ]] ||
  fail "direct cohort publishing requires confirmation text publish"

expected="$(release_set_canonical "$EXPECTED_RELEASE_SET")" ||
  fail "expected release set is invalid"
release_sha="$(printf '%s' "$RELEASE_SHA" | tr '[:upper:]' '[:lower:]')"
[[ "$release_sha" =~ ^[0-9a-f]{40}$ ]] ||
  fail "RELEASE_SHA must be a full 40-character hexadecimal commit SHA"
[[ "$(git -C "$ROOT" rev-parse HEAD)" == "$release_sha" ]] ||
  fail "checked-out source does not match RELEASE_SHA"

metadata="$(cd "$ROOT" && cargo metadata --locked --no-deps --format-version 1)" ||
  fail "cargo metadata failed for the release source"
publishable_filter='(.publish == null or ((.publish | type) == "array" and (.publish | index("crates-io") != null)))'

packages=()
versions=()
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

if (( ${#packages[@]} == 0 )); then
  printf 'No unpublished crates were approved for this release.\n'
  exit 0
fi

(cd "$ROOT" && cargo fetch --locked) ||
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

scratch="$(mktemp -d "${TMPDIR:-/tmp}/lenso-runtime-publish.XXXXXX")" ||
  fail "could not create a temporary package directory"
scratch="$(cd -- "$scratch" && pwd -P)"
cleanup() {
  rm -rf -- "$scratch"
}
trap cleanup EXIT

completed_packages=()
published_records='[]'
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

    version="${versions[$index]}"
    package_target="$scratch/$package"
    (
      cd "$ROOT"
      cargo package --locked --no-verify --target-dir "$package_target" -p "$package"
    ) || fail "could not package ${package}@${version} from the exact release source"
    artifact="$package_target/package/$package-$version.crate"
    [[ -f "$artifact" ]] || fail "cargo package did not produce $package-$version.crate"
    digest="$(sha256_file "$artifact")"
    [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || fail "could not calculate the SHA-256 digest for $package"

    if existing_checksum="$(registry_checksum "$package" "$version")"; then
      [[ "$existing_checksum" == "$digest" ]] ||
        fail "crates.io already has a different artifact for ${package}@${version}"
      printf 'Already visible with matching checksum: %s@%s\n' "$package" "$version"
    else
      if ! (cd "$ROOT" && cargo publish --locked --no-verify -p "$package"); then
        # A failed command can still represent an unknown external mutation.
        wait_for_registry_checksum "$package" "$version" "$digest"
      else
        wait_for_registry_checksum "$package" "$version" "$digest"
      fi
    fi

    completed_packages+=("$package")
    published_records="$(jq -c --arg package "$package" --arg version "$version" --arg digest "$digest" \
      '. + [{package_name: $package, version: $version, sha256: $digest}]' <<<"$published_records")"
    made_progress=true
  done
  [[ "$made_progress" == true ]] ||
    fail "could not derive a topological order for the approved release cohort"
done

actual_release_set="$(jq -c '[.[] | {package_name, version}] | sort_by(.package_name)' <<<"$published_records")"
[[ "$actual_release_set" == "$expected" ]] ||
  fail "published cohort does not match the approved release set"

printf 'Cohort publish completed: %s\n' "$published_records"
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    printf '### Exact cohort publication\n\n'
    printf -- '- Source: `%s`\n' "$release_sha"
    printf -- '- Each crate used direct Cargo OIDC publishing after artifact verification.\n'
    printf -- '- Registry visibility and checksum were confirmed before dependent crates.\n'
    jq -r '.[] | "- `\(.package_name)@\(.version)`: SHA-256 `\(.sha256)`"' <<<"$published_records"
  } >>"$GITHUB_STEP_SUMMARY"
fi
