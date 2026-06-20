#!/usr/bin/env sh
set -eu

for package in $(sh "$(dirname "$0")/publish-crate-order.sh"); do
    cargo pkgid -p "$package" >/dev/null
done

echo "Dry-running cargo package for lenso contracts..."
cargo package --locked -p lenso-contracts --allow-dirty

echo "Downstream host crates are dry-run verified one-by-one during staged publish."
echo "Package readiness checks passed."
