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
artifact_readme="$out_dir/lenso-$version-artifact-readme.md"
notes_file="$out_dir/lenso-$version-release-notes.md"
source_archive="$out_dir/lenso-$version-source.tar.gz"
summary="${LENSO_RELEASE_NOTES_SUMMARY:-First Lenso release candidate.}"
commit_sha="$(git rev-parse HEAD)"

mkdir -p "$out_dir"
rm -f "$artifact_readme" "$notes_file" "$source_archive"

git archive \
    --format=tar.gz \
    --prefix="lenso-$version/" \
    --output="$source_archive" \
    HEAD

cat > "$notes_file" <<EOF
# Lenso $version

## Summary

$summary

## Release Inputs

- Commit: $commit_sha
- Gate: \`just release-check\` passed in the release workflow.
- Runtime Console checks run in the separate \`lenso-runtime-console\` repository.

## First Release Scope

- Linked modules load through the app bootstrap composition root.
- Remote modules install through \`lenso module add <manifest-url>\`.
- Remote module manifests can declare schema-admin, HTTP routes, runtime functions, and lifecycle activation jobs.
- Runtime Console integration is provided by the separate \`lenso-runtime-console\` repository.
- Generated contracts and the TypeScript SDK are committed and reproducible.

## Getting Started

\`\`\`sh
just install
just db-up
just migrate
just check
\`\`\`

## Known Caveats

- Local service smoke still requires Postgres and separate API, worker, and Runtime Console shells.
- Remote module install is intentionally decentralized and low-friction.
- Publisher trust, registry review, install history, doctor flows, bundle import/export, provenance, and signatures are not release blockers.
EOF

cat > "$artifact_readme" <<EOF
# Lenso $version Artifacts

This release workflow uploads three artifacts:

- \`lenso-$version-release-notes.md\`: draft notes for the GitHub Release body.
- \`lenso-$version-source.tar.gz\`: a source archive generated from \`git archive HEAD\`.
- \`lenso-$version-artifact-readme.md\`: this artifact guide.

The source archive contains repository source files, committed contracts,
examples, docs, and scripts. It does not include local build output, \`.git\`,
\`target/\`, \`node_modules/\`, or \`dist/\`.

After extracting the source archive:

\`\`\`sh
cd lenso-$version
just install
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
