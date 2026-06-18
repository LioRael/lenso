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
lenso_contracts_pkgid="$(cargo pkgid -p lenso-contracts)"
lenso_contracts_version="${lenso_contracts_pkgid##*#}"

if [ "$lenso_crate_version" != "$package_version" ]; then
    echo "lenso crate version $lenso_crate_version does not match $version" >&2
    exit 1
fi

if [ "$lenso_contracts_version" != "$package_version" ]; then
    echo "lenso-contracts crate version $lenso_contracts_version does not match $version" >&2
    exit 1
fi

echo "Release version $version matches lenso and lenso-contracts crate metadata."
