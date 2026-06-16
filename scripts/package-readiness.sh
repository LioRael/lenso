#!/usr/bin/env sh
set -eu

echo "Dry-running cargo package for lenso facade..."
cargo package --locked -p lenso --allow-dirty

echo "Package readiness checks passed."
