#!/usr/bin/env bash
set -euo pipefail
repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
: "${LENSO_JS_ROOT:?Pass the exact qualified JS source checkout}"
source "$repo/.github/scripts/check.sh"
verify_js_revision
export PYTHONDONTWRITEBYTECODE=1
cargo build --locked --manifest-path "$repo/Cargo.toml" -p lenso-cli --bin lenso
cli="${CARGO_TARGET_DIR:-$repo/target}/debug/lenso"
cli="$(realpath "$cli")"
output="$repo/.lenso/source-app-proof"
mkdir -p "$output"
cd "$repo/examples/onboarding/source-first-instances"
"$cli" app build --out "$output/rust-native"
"$cli" app build --target workers --out "$output/rust-workers" \
  --workers-runtime "$LENSO_JS_ROOT/packages/lenso-workers-runtime" \
  --wasm-bindgen "$repo/.lenso/ci-tools/bin/wasm-bindgen"
python3 verify.py --cli "$cli" --native "$output/rust-native" --workers "$output/rust-workers" \
  --workerd "$repo/.lenso/ci-tools/node_modules/@cloudflare/workerd-linux-64/bin/workerd" \
  --output "$output/rust-verification.json"
LENSO_SOURCE_INSTANCE_DISTRIBUTION="$output/rust-native" \
  cargo test --locked --manifest-path "$repo/Cargo.toml" -p lenso-source-first-instances --test simulated -- --ignored
cd "$repo/examples/onboarding/source-first-mixed"
trap 'git restore -- package.json' EXIT
node prepare-source-sdk.mjs "$LENSO_JS_ROOT"
bun run check
"$cli" app build --out "$output/native"
"$cli" app build --target workers --out "$output/workers" \
  --workers-runtime "$LENSO_JS_ROOT/packages/lenso-workers-runtime" \
  --wasm-bindgen "$repo/.lenso/ci-tools/bin/wasm-bindgen"
python3 verify.py --cli "$cli" --native "$output/native" --workers "$output/workers" \
  --workerd "$repo/.lenso/ci-tools/node_modules/@cloudflare/workerd-linux-64/bin/workerd" \
  --output "$output/verification.json"
