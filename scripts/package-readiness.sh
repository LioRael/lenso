#!/usr/bin/env sh
set -eu

echo "Packaging the public Rust facade and contract crates..."
cargo package --locked -p lenso-contracts --allow-dirty
cargo package --locked -p lenso --allow-dirty

echo "Checking the public TypeScript Service Kit..."
pnpm --dir sdk/typescript check

echo "Package readiness checks passed."
