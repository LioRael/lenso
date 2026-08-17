#!/usr/bin/env bash
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
target_directory="${CARGO_TARGET_DIR:-$repository_root/target}"
metadata="$(cargo metadata --no-deps --format-version 1)"

package_version() {
  jq -r --arg package_name "$1" \
    '.packages[] | select(.name == $package_name) | .version' <<<"$metadata"
}

package_root() {
  local manifest_path
  manifest_path="$(jq -r --arg package_name "$1" \
    '.packages[] | select(.name == $package_name) | .manifest_path' <<<"$metadata")"
  dirname "$manifest_path"
}

# The lightweight linked-module facade and its internal dependencies ship as
# one coordinated release closure. Build local archives so this pre-release
# consumer never falls back to an older registry implementation while still
# authoring against `lenso` alone.
closure_packages=(
  lenso-platform-core
  lenso-platform-runtime
  lenso-platform-module
)

# `cargo package` normalizes path dependencies back to registry dependencies.
# Resolve the exact coordinated closure from the workspace while creating the
# archives so the extracted consumer sees the changed local packages.
package_patch_arguments=()
for package_name in "${closure_packages[@]}"; do
  package_patch_arguments+=(
    --config
    "patch.crates-io.$package_name.path=\"$(package_root "$package_name")\""
  )
done
for package_name in "${closure_packages[@]}"; do
  cargo package --locked --allow-dirty --no-verify -p "$package_name" \
    "${package_patch_arguments[@]}"
done
cargo package --locked --allow-dirty --no-verify -p lenso --features linked-module \
  "${package_patch_arguments[@]}"

fixture_root="$repository_root/crates/lenso/tests/fixtures/linked-runtime-consumer"
function_contract="$fixture_root/contracts/runtime/functions/fixture.reconcile.v1.schema.json"
test "$(jq -r '."$id"' "$function_contract")" = "fixture.reconcile.v1"
test "$(jq -r '.title' "$function_contract")" = "fixture.reconcile.v1"
temporary_root="$(mktemp -d)"
trap 'rm -rf "$temporary_root"' EXIT

for package_name in "${closure_packages[@]}" lenso; do
  version="$(package_version "$package_name")"
  tar -xzf "$target_directory/package/$package_name-$version.crate" -C "$temporary_root"
done

lenso_version="$(package_version lenso)"
consumer_root="$temporary_root/consumer"
mkdir -p "$consumer_root/src"
sed "s|__LENSO_PACKAGE_PATH__|$temporary_root/lenso-$lenso_version|" \
  "$fixture_root/Cargo.toml.template" > "$consumer_root/Cargo.toml"
cp "$fixture_root/src/main.rs" "$consumer_root/src/main.rs"
mkdir -p "$consumer_root/contracts/runtime/functions"
cp "$function_contract" \
  "$consumer_root/contracts/runtime/functions/fixture.reconcile.v1.schema.json"

{
  printf '\n[patch.crates-io]\n'
  for package_name in "${closure_packages[@]}"; do
    version="$(package_version "$package_name")"
    printf '%s = { path = "%s/%s-%s" }\n' \
      "$package_name" "$temporary_root" "$package_name" "$version"
  done
} >> "$consumer_root/Cargo.toml"

cargo generate-lockfile --manifest-path "$consumer_root/Cargo.toml"
cargo check --locked --manifest-path "$consumer_root/Cargo.toml"
