#!/usr/bin/env sh
set -eu

echo "Dry-running cargo package for lenso contracts..."
cargo package --locked -p lenso-contracts --allow-dirty

echo "Package readiness checks passed."
