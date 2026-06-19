#!/usr/bin/env sh
set -eu

mode="${1:-publish}"
case "$mode" in
    publish | --dry-run) ;;
    *)
        echo "usage: $0 [--dry-run]" >&2
        exit 2
        ;;
esac

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
packages="$(sh "$script_dir/publish-crate-order.sh")"

crate_version() {
    cargo pkgid -p "$1" | sed 's/.*#//' | sed 's/.*@//'
}

crate_version_exists() {
    crate="$1"
    version="$2"
    curl -A 'lenso-release-publish' -fsS \
        "https://crates.io/api/v1/crates/$crate/$version" >/dev/null 2>&1
}

wait_for_crate_version() {
    crate="$1"
    version="$2"
    for _ in 1 2 3 4 5 6 7 8 9 10 11 12; do
        if crate_version_exists "$crate" "$version"; then
            return 0
        fi
        sleep 10
    done
    echo "timed out waiting for $crate $version to appear on crates.io" >&2
    return 1
}

for package in $packages; do
    version="$(crate_version "$package")"
    if [ "$mode" = "--dry-run" ]; then
        echo "Dry-running cargo publish for $package $version..."
        cargo publish --dry-run --locked -p "$package" --allow-dirty
    elif crate_version_exists "$package" "$version"; then
        echo "$package $version is already on crates.io; skipping."
    else
        echo "Dry-running cargo publish for $package $version..."
        cargo publish --dry-run --locked -p "$package" --allow-dirty
        echo "Publishing $package $version to crates.io..."
        cargo publish --locked -p "$package" --allow-dirty
        wait_for_crate_version "$package" "$version"
    fi
done
