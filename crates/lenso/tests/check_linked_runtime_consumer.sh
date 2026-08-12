#!/usr/bin/env bash
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
target_directory="${CARGO_TARGET_DIR:-$repository_root/target}"
package_version="$(
  cargo metadata --no-deps --format-version 1 |
    jq -r '.packages[] | select(.name == "lenso") | .version'
)"

cargo package --locked --allow-dirty -p lenso --features host

fixture_root="$repository_root/crates/lenso/tests/fixtures/linked-runtime-consumer"
temporary_root="$(mktemp -d)"
trap 'rm -rf "$temporary_root"' EXIT

tar -xzf "$target_directory/package/lenso-$package_version.crate" -C "$temporary_root"
consumer_root="$temporary_root/consumer"
mkdir -p "$consumer_root/src"
sed "s|__LENSO_PACKAGE_PATH__|$temporary_root/lenso-$package_version|" \
  "$fixture_root/Cargo.toml.template" > "$consumer_root/Cargo.toml"
cp "$fixture_root/src/main.rs" "$consumer_root/src/main.rs"

cargo generate-lockfile --manifest-path "$consumer_root/Cargo.toml"
cargo check --locked --manifest-path "$consumer_root/Cargo.toml"
