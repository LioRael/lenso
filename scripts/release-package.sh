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
        echo "LENSO_RELEASE_VERSION must start with v, for example v0.2.0" >&2
        exit 1
        ;;
esac

repo_root="$(git rev-parse --show-toplevel)"
out_dir="$repo_root/dist/release"
artifact_readme="$out_dir/lenso-$version-artifact-readme.md"
notes_file="$out_dir/lenso-$version-release-notes.md"
source_archive="$out_dir/lenso-$version-source.tar.gz"
hosted_archive="$out_dir/lenso-$version-hosted.tar.gz"
console_dist_dir="${LENSO_CONSOLE_DIST_DIR:-"$repo_root/.lenso/console/dist"}"
console_extensions_dir="${LENSO_CONSOLE_EXTENSIONS_DIR:-"$repo_root/.lenso/console/extensions"}"
summary="${LENSO_RELEASE_NOTES_SUMMARY:-First Lenso release candidate.}"
commit_sha="$(git rev-parse HEAD)"

mkdir -p "$out_dir"
rm -f "$artifact_readme" "$notes_file" "$source_archive" "$hosted_archive"

git archive \
    --format=tar.gz \
    --prefix="lenso-$version/" \
    --output="$source_archive" \
    HEAD

hosted_archive_line="- Hosted Runtime Console archive skipped because \`$console_dist_dir/index.html\` was not present."
hosted_console_status="not included"
if [ -f "$console_dist_dir/index.html" ]; then
    tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/lenso-release.XXXXXX")"
    trap 'rm -rf "$tmp_dir"' EXIT INT TERM
    tar -xzf "$source_archive" -C "$tmp_dir"
    release_root="$tmp_dir/lenso-$version"

    mkdir -p "$release_root/.lenso/console"
    cp -R "$console_dist_dir" "$release_root/.lenso/console/dist"

    mkdir -p "$release_root/.lenso/console/extensions"
    if [ -d "$console_extensions_dir" ]; then
        cp -R "$console_extensions_dir/." "$release_root/.lenso/console/extensions"
    fi
    if [ ! -f "$release_root/.lenso/console/extensions/registry.json" ]; then
        printf '{"version":1,"bundles":[]}\n' >"$release_root/.lenso/console/extensions/registry.json"
    fi

    (cd "$tmp_dir" && tar -czf "$hosted_archive" "lenso-$version")
    hosted_archive_line="- \`lenso-$version-hosted.tar.gz\`: source archive plus hosted Runtime Console dist under \`.lenso/console/dist\`."
    hosted_console_status="included in \`lenso-$version-hosted.tar.gz\`"
fi

cat > "$notes_file" <<EOF
# Lenso $version

## Summary

$summary

## Release Inputs

- Commit: $commit_sha
- Gate: \`just release-check\` passed in the release workflow.
- Package preflight: \`just package-readiness\` passed in the release workflow.
- Registry uploads: controlled by the release workflow publish inputs.
- Runtime Console checks run in the separate \`lenso-runtime-console\` repository.
- Hosted Runtime Console dist: $hosted_console_status.

## First Release Scope

- Linked modules load through the app bootstrap composition root.
- Remote modules install through \`lenso module add <manifest-url>\`.
- Remote module manifests can declare schema-admin, HTTP routes, runtime functions, and lifecycle activation jobs.
- Runtime Console is served by the API from the hosted release artifact.
- Generated contracts are committed and reproducible.

## Getting Started

\`\`\`sh
just db-up
just migrate
just check
\`\`\`

## Known Caveats

- Source-only local smoke still requires Postgres and separate API, worker, and Runtime Console shells.
- Remote module install is intentionally decentralized and low-friction.
- Publisher trust, registry review, install history, doctor flows, bundle import/export, provenance, and signatures are not release blockers.
EOF

cat > "$artifact_readme" <<EOF
# Lenso $version Artifacts

This release workflow uploads these artifacts:

- \`lenso-$version-release-notes.md\`: draft notes for the GitHub Release body.
- \`lenso-$version-source.tar.gz\`: a source archive generated from \`git archive HEAD\`.
$hosted_archive_line
- \`lenso-$version-artifact-readme.md\`: this artifact guide.

The source archive contains repository source files, committed contracts,
examples, docs, and scripts. It does not include local build output, \`.git\`,
\`target/\`, or \`dist/\`. The hosted archive additionally includes the
prebuilt Runtime Console static files; running it does not require Node.js or
pnpm.

After extracting either archive:

\`\`\`sh
cd lenso-$version
cp .env.example .env
just db-up
just migrate
just check
\`\`\`

For the full release gate:

\`\`\`sh
just release-check
\`\`\`
EOF

echo "Artifact README: $artifact_readme"
echo "Release notes: $notes_file"
echo "Source package: $source_archive"
