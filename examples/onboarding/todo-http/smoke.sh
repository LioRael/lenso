#!/usr/bin/env bash
set -euo pipefail

example_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$example_dir/../../.." && pwd)
target_dir=${CARGO_TARGET_DIR:-$repo_root/target}
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
cargo build --locked --manifest-path "$repo_root/Cargo.toml" \
  --target-dir "$target_dir" -p lenso-onboarding-todo-http --bin lenso-onboarding-todo-http
python3 "$example_dir/smoke.py" "$target_dir/debug/lenso-onboarding-todo-http"
