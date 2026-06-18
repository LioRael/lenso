#!/usr/bin/env sh
set -eu

echo "Dry-running cargo package for lenso facade..."
cargo package --locked -p lenso --allow-dirty

echo "Dry-running cargo package for lenso-cli..."
cargo package --locked -p lenso-cli --allow-dirty

echo "Package readiness checks passed."
