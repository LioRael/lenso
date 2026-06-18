#!/usr/bin/env sh
set -eu

version="${LENSO_RELEASE_VERSION:-}"

if [ -z "$version" ]; then
    echo "LENSO_RELEASE_VERSION is required" >&2
    exit 1
fi

case "$version" in
    v[0-9]*.[0-9]*.[0-9]*) ;;
    *)
        echo "LENSO_RELEASE_VERSION must look like v0.2.0" >&2
        exit 1
        ;;
esac

package_version="${version#v}"
lenso_pkgid="$(cargo pkgid -p lenso)"
lenso_crate_version="${lenso_pkgid##*#}"

if [ "$lenso_crate_version" != "$package_version" ]; then
    echo "lenso crate version $lenso_crate_version does not match $version" >&2
    exit 1
fi

if grep -R 'branch = "main"' crates/lenso-cli/templates/starter-host/Cargo.toml.tmpl >/dev/null; then
    echo "starter host must not depend on branch = main for release" >&2
    exit 1
fi

echo "Release version $version matches lenso crate metadata."
