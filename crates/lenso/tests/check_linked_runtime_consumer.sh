#!/usr/bin/env bash
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
target_directory="${CARGO_TARGET_DIR:-$repository_root/target}"
metadata="$(cargo metadata --no-deps --format-version 1)"

package_version() {
  jq -r --arg package_name "$1" \
    '.packages[] | select(.name == $package_name) | .version' <<<"$metadata"
}

cargo package --locked --allow-dirty -p lenso --features host

# The public facade change and the Host validation behind it ship as one
# coordinated release closure. Build local archives for every changed internal
# dependency so this pre-release consumer never falls back to an older registry
# implementation while still authoring against `lenso` alone.
closure_packages=(
  lenso-platform-core
  lenso-platform-runtime
  lenso-platform-module
  lenso-platform-provider
  lenso-bootstrap
  lenso-worker
)
for package_name in "${closure_packages[@]}"; do
  cargo package --locked --allow-dirty --no-verify -p "$package_name"
done

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
pinned_packages=(
  lenso-api
  lenso-migrate
  lenso-worker
  lenso-bootstrap
  lenso-platform-module-management
  lenso-platform-runtime-observability
  lenso-platform-runtime-operations
  lenso-platform-system-plane
  lenso-platform-provider
  lenso-platform-module
  lenso-platform-http
  lenso-platform-runtime
  lenso-platform-core
)
for package_name in "${pinned_packages[@]}"; do
  cargo update --manifest-path "$consumer_root/Cargo.toml" \
    -p "$package_name" --precise "$(package_version "$package_name")"
done
cargo check --locked --manifest-path "$consumer_root/Cargo.toml"
