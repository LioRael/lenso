#!/usr/bin/env sh
set -eu

cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
just generate-contracts
just generate-ts-sdk
cargo run -p arch-check
pnpm --dir packages/ts-sdk typecheck
