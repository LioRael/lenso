#!/usr/bin/env sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
version="${LENSO_RELEASE_VERSION:-}"

if [ -z "$version" ]; then
    echo "LENSO_RELEASE_VERSION is required" >&2
    exit 1
fi

case "$version" in
    v[0-9]*.[0-9]*.[0-9]*) ;;
    *)
        echo "LENSO_RELEASE_VERSION must look like v0.1.0" >&2
        exit 1
        ;;
esac

package_version="${version#v}"
metadata_json="$(mktemp)"

cleanup() {
    rm -f "$metadata_json"
}
trap cleanup EXIT

ts_sdk_version="$(node - "$root_dir/packages/ts-sdk/package.json" <<'NODE'
const fs = require("node:fs");

const manifestPath = process.argv[2];
const pkg = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
process.stdout.write(pkg.version);
NODE
)"

cargo metadata --format-version=1 --no-deps --locked > "$metadata_json"
lenso_crate_version="$(node - "$metadata_json" <<'NODE'
const fs = require("node:fs");

const metadataPath = process.argv[2];
const metadata = JSON.parse(fs.readFileSync(metadataPath, "utf8"));
const crate = metadata.packages.find((pkg) => pkg.name === "lenso");
if (!crate) {
  console.error("crate `lenso` was not found in cargo metadata");
  process.exit(1);
}
process.stdout.write(crate.version);
NODE
)"

if [ "$ts_sdk_version" != "$package_version" ]; then
    echo "@lenso/ts-sdk version $ts_sdk_version does not match $version" >&2
    exit 1
fi

if [ "$lenso_crate_version" != "$package_version" ]; then
    echo "lenso crate version $lenso_crate_version does not match $version" >&2
    exit 1
fi

echo "Release version $version matches @lenso/ts-sdk and lenso crate metadata."
