#!/usr/bin/env bash
# One bounded focused observation; never rerun until green or change timeouts.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd "$root"
export RUSTUP_TOOLCHAIN=1.94.0
: "${1:?Usage: diagnose-lane-shutdown.sh <new-evidence-directory>}"
test ! -e "$1"
mkdir -p "$1"
evidence="$(cd "$1" && pwd -P)"
{
  printf 'sha=%s\nos=%s\n' "$(git rev-parse HEAD)" "$(uname -sm)"
  printf 'github_run=%s attempt=%s\n' "${GITHUB_RUN_ID:-local}" "${GITHUB_RUN_ATTEMPT:-local}"
  rustc --version --verbose
  cargo --version
  git status --short
  printf '%s\n' 'cargo test --locked -p lenso-runner --test replicated_interactions plugin_lifecycle_preserves_cross_lane_stream_protocol_and_event_fanout -- --exact --nocapture --test-threads=1'
} >"$evidence/inputs.txt"
status=0
cargo test --locked -p lenso-runner --test replicated_interactions \
  plugin_lifecycle_preserves_cross_lane_stream_protocol_and_event_fanout \
  -- --exact --nocapture --test-threads=1 >"$evidence/test.log" 2>&1 || status=$?
printf '%s\n' "$status" >"$evidence/exit-status.txt"
cat "$evidence/test.log"
exit "$status"
