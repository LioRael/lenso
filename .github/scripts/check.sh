#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd "$root"
export RUSTUP_TOOLCHAIN=1.94.0
export PYTHONDONTWRITEBYTECODE=1
run() { printf '+ '; printf '%q ' "$@"; printf '\n'; "$@"; }
preflight() {
  run python3 .github/scripts/check-fixture-inputs.py
  run python3 .github/scripts/test-check-fixture-inputs.py
  run cargo fmt --all -- --check
  run bash .github/scripts/test-release-cohort-preflight.sh
  run bash .github/scripts/test-release-scripts.sh
}
native() {
  # Existing contract/scaffold assertions now fail before all-target Clippy.
  run cargo test --locked -p lenso-engine-app --lib plugin::tests
  run cargo clippy --locked --workspace --all-targets --timings -- -D warnings
  run cargo test --locked --workspace --no-run --timings
  run cargo test --locked --workspace
}
wasm() {
  run cargo check --locked --timings -p lenso-app-plan -p lenso-kernel -p lenso-runtime-conformance --target wasm32-unknown-unknown
  run cargo check --locked --timings -p lenso-workers-http-parity-host --target wasm32-unknown-unknown
  run cargo check --locked --timings -p lenso-http-egress-plugin --no-default-features --features workers --target wasm32-unknown-unknown
  run cargo check --locked --timings -p lenso-app-plan -p lenso-kernel -p lenso-runtime-conformance --target wasm32-wasip2
}
bun_inputs() {
  : "${LENSO_JS_ROOT:?Set LENSO_JS_ROOT to the pinned lenso-js fixture checkout}"
  test "$(bun --version)" = 1.4.2
  test "$(node --version)" = v24.18.0
  test "$(git -C "$LENSO_JS_ROOT" rev-parse HEAD)" = 18e3cfb2837c8dfe5d5b907e39fe95ae15dc0a65
}
bun_commands() {
  (cd "$LENSO_JS_ROOT"; run bun install --frozen-lockfile; run bun run build)
  run cargo test --locked -p lenso-bun-adapter --features js-integration --test authoring_v2 --test bun_cross_runtime --test process_v1_bootstrap -- --include-ignored --test-threads=1
  run node --test crates/lenso-engine-app/tests/plugin-build-symbols.test.mjs
}
check_bun() {
  bun_inputs
  run python3 .github/scripts/test-check-bun.py
  bun_commands
}
distribution() (
  : "${EXPECTED_RELEASE_SET:?Supply the exact approved package_name/version JSON release set}"
  : "${RELEASE_SHA:?Supply the exact full candidate SHA}"
  test -z "$(git status --porcelain)"
  local distribution_root
  distribution_root="$(mktemp -d "${TMPDIR:-/tmp}/lenso-candidate-consumer.XXXXXX")"
  trap 'rm -rf -- "$distribution_root"' EXIT
  mkdir -p "$distribution_root/home" "$distribution_root/cargo-home"
  run env HOME="$distribution_root/home" CARGO_HOME="$distribution_root/cargo-home" \
    RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" bash .github/scripts/release-cohort-preflight.sh
)
main() {
  local phase="${1:-all}"
  case "$phase" in all|preflight|native|wasm|bun|distribution) ;; *) echo "Unknown check phase: $phase" >&2; return 2 ;; esac
  printf 'Lenso check: sha=%s phase=%s os=%s\n' "$(git rev-parse HEAD)" "$phase" "$(uname -sm)"
  git status --short
  rustc --version
  cargo --version
  case "$phase" in
    all) bun_inputs; preflight; native; wasm; check_bun ;;
    bun) check_bun ;;
    *) "$phase" ;;
  esac
}
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then main "$@"; fi
