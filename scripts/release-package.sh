#!/usr/bin/env sh
set -eu

version="${LENSO_RELEASE_VERSION:-}"
if [ -z "$version" ]; then
    echo "LENSO_RELEASE_VERSION is required" >&2
    exit 1
fi

case "$version" in
    v*) ;;
    *)
        echo "LENSO_RELEASE_VERSION must start with v, for example v0.1.0" >&2
        exit 1
        ;;
esac

out_dir="dist/release"
package_root="$out_dir/lenso-$version"
notes_file="$out_dir/lenso-$version-release-notes.md"
source_archive="$out_dir/lenso-$version-source.tar.gz"
summary="${LENSO_RELEASE_NOTES_SUMMARY:-First Lenso release candidate.}"
commit_sha="$(git rev-parse HEAD)"

rm -rf "$package_root"
mkdir -p "$package_root" "$out_dir"

cp README.md "$package_root/"
cp justfile "$package_root/"
mkdir -p "$package_root/docs" "$package_root/examples/remote-modules"
cp -R docs/. "$package_root/docs/"
cp -R examples/remote-modules/. "$package_root/examples/remote-modules/"

cat > "$notes_file" <<EOF
# Lenso $version

## Summary

$summary

## Release Inputs

- Commit: $commit_sha
- Gate: \`just release-check\` passed in the release workflow.
- Demo: \`just demo-release\` is included in the gate.

## First Release Scope

- Linked modules load through the app bootstrap composition root.
- Remote modules install through \`lenso module add <manifest-url>\`.
- Remote module manifests can declare schema-admin, HTTP routes, runtime functions, and lifecycle activation jobs.
- Runtime Console shows loaded modules, remote calls, runtime functions, and lifecycle activation declarations.
- Generated contracts and the TypeScript SDK are committed and reproducible.

## Getting Started

\`\`\`sh
just install
just db-up
just migrate
just demo-release
\`\`\`

## Known Caveats

- Local service smoke still requires Postgres and separate API, worker, and Console shells.
- Remote module install is intentionally decentralized and low-friction.
- Publisher trust, registry review, install history, doctor flows, bundle import/export, provenance, and signatures are not release blockers.
EOF

tar -czf "$source_archive" -C "$out_dir" "lenso-$version"

echo "Release notes: $notes_file"
echo "Source package: $source_archive"
