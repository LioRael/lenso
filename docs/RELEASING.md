# Local release preparation

This workflow does not use CI and never publishes, pushes, creates Git tags or
creates remote releases. Publishing is a separate human command after approval.
Use the repository's Bun version (`1.4.2`) and installed dependencies. `tar` must
be available on PATH.

## Decide before a first publication

The repository does not establish a registry, ownership of the unscoped names
`lenso` and `lenso-cli` or the `@lenso` scope, package access, license, or dist-tag.
Confirm those with the release owner. Do not infer a registry from a package
name. Review legal/package metadata before publication; the current manifests
do not declare a license. Local checks cannot prove name availability or
permission to publish.

## Record changes and prepare versions

Changesets owns version calculation, changelogs and internal dependency updates.
`@changesets/cli` is pinned to stable `3.0.3`, not GitHub `main` or a `next`
prerelease. The official [npm metadata](https://registry.npmjs.org/@changesets/cli/3.0.3)
and [release](https://github.com/changesets/changesets/releases/tag/%40changesets/cli%403.0.3)
identify this as a published, non-prerelease version. The package declares Node
`^22.11 || ^24 || >=26` for Node execution. Our scripts explicitly run the installed
CLI with Bun `1.4.2`; local status and fixture version tests exercise that path.
These checks are not a claim of official Bun support for every Changesets command.

From the repository root, record a source change and preview it:

```sh
bun install --frozen-lockfile
bun run changeset
bun run release:status
# Optional machine-readable plan, not a verification receipt:
mkdir -p output/release
bun run release:status --output output/release/status.json
```

`changeset` is an alias for `changeset add`, not an unrestricted release command.
Select the affected public packages, SemVer bump types and changelog summary.
Commit the new `.changeset/*.md` with its source change. For changes that need no
package release, no changeset is necessary; there is no CI requiring empty files.
However, Changesets status exits 1 if it detects changed packages with no pending
changesets. For an intentional no-release batch, `bun run changeset --empty`
records that decision and lets status succeed without planning a version bump.
Status reads pending changesets without bumping repository versions or querying
a registry. It is a version plan, not a list of unpublished remote versions.
The configured base branch is `main`; it must exist locally for change detection.
Use `--since <local-ref>` when preparing against a different base.

When ready to prepare a release, inspect the plan **before** consuming it:

```sh
bun run release:status
bun run release:version
bun install
bun install --frozen-lockfile
bun run fmt
```

`release:version` changes package manifests and changelogs and consumes the
changeset files. It does not publish, commit, push or tag. Run it on a reviewed
working tree and review all resulting edits before committing. Changesets does
not update `bun.lock`; the integration owner runs `bun install` at the root and
reviews its lockfile diff together with the manifests. Do not use frozen install
as the update step, install inside individual packages, or introduce another
package-manager lockfile. See Bun's official [workspaces](https://bun.sh/docs/pm/workspaces)
and [lockfile](https://bun.sh/docs/pm/lockfile) documentation.

`.changeset/config.json` uses independent stable versions (`fixed` and `linked`
are empty), no automatic commits, and no private package versions or tags.
The private root and all `examples/*` are excluded from versioning; templates and
nested CLI example plugins are outside the root workspace globs and are not
release candidates. `ignore` is empty because Changesets documents it as a
temporary pause, not a permanent private-package exclusion. `format: false`
leaves formatting to the repository's `fmt` command.
The JSON status plan can include private dependents with `type: "none"` and an
unchanged `newVersion`; these are dependency bookkeeping, not package releases.

Internal dependencies, including explicit peers such as `@lenso/auth`'s `lenso`
range, remain Changesets-owned (`bumpVersionsWithWorkspaceProtocolOnly: false`,
`updateInternalDependencies: "patch"`). Review the resulting plan: independent
versions do not mean dependent packages can never need a release. Workspace
placeholders may remain in source; Bun pack translates `workspace:^` and
`workspace:~` using the target's prepared version. Do not manually propagate
versions with another algorithm. See the official Changesets [configuration](https://changesets.dev/guide/config)
and [CLI](https://changesets.dev/guide/cli) documentation.

Registry, access and tag are still unconfirmed. The omitted Changesets `access`
field retains the built-in `restricted` fallback; it is not our approved access
policy. No publication script is provided. Do not use prerelease or snapshot mode
without an explicit release policy, and never send a prerelease to `latest`
accidentally.

## Validate locally

From the repository root:

```sh
bun install --frozen-lockfile
bun test scripts/release.test.ts
bun run typecheck
bun run lint
bun run fmt:check
bun run release:verify
bun run test
```

The archive verifier only reads direct `packages/*/package.json` manifests.
The private root, `examples/*`, `templates/*` and nested CLI example plugins are
not independent release candidates. A direct package with `private: true` is
excluded. CLI example source files remain part of `lenso-cli`'s existing allowlist.

The verifier (formerly `release:check`):

1. Derives package names and dependency-first order from manifests, including
   local peers and development edges needed by builds. Detects cycles, missing
   workspace targets, duplicate names and runtime edges to private packages.
2. Builds each public framework package using its existing build script before
   packing. This is a real local build, not a no-write simulation.
3. Runs `bun pm pack --ignore-scripts --filename <archive>` in each package.
   Bun replaces `workspace:^` with the target package's version range. Source
   manifests are not rewritten; lifecycle pack/publish scripts are not run.
4. Reads the actual archive's root manifest and file list. Checks identity,
   `dist`, declared exports (including wildcard migrations), types and bin
   entry points, CLI shebang, unresolved runtime `workspace:`/`file:`/`link:`
   references, credential files, `.lenso`, `node_modules` and nested tarballs.
5. Writes archives and a `verified.json` receipt with ordered package names,
   versions, full file lists and SHA-256 hashes in a unique ignored
   `output/release/<run>/` directory. Only a fully successful run gets a receipt.

Review the complete file lists for unintended or sensitive content. This
filename check is not a content-level secret scanner. Archive validation does
not prove all imports, peer compatibility or remote dependency availability.
Build scripts are trusted repository code. No login or registry request is
needed for verification. This script only checks the prepared versions; it does
not calculate version bumps, rewrite manifests or generate changelogs.

The full tests include the existing standalone packed Engine/CLI consumer and
template workflows. They install dependencies and may use the configured
registry, but do not publish. Templates intentionally consume local `vendor`
tarballs; this release workflow does not change them to remote dependencies.
Verification itself does not run the full test suite.

## Publish only after explicit approval

The recommended publication path remains manual publication of the exact
`release:verify` tarballs below. Changesets is used for `add`, `status` and
`version`, not publication. Plain `changeset publish` would pack from mutable
package directories and create Git tags by default, so it is not equivalent to
publishing our verified artifacts. Changesets 3 also offers `pack` and
`publish --from-pack-dir`, but this repository does not configure or verify their
separate output format. Do not treat `verified.json` as a Changesets pack
manifest, substitute a fresh pack, or execute any publication without separate
human authorization.

Do not run these commands as part of local preparation. After the registry,
ownership, versions, license, access and tag are confirmed and publication is
authorized, the human release operator uses their normal external npm credential
store (and interactive OTP if requested). Never commit `.npmrc`, tokens or
credential files to this repository, or put tokens in command arguments.

Set the following variables to the approved values in your own shell. There
are intentionally no registry, tag or access defaults:

```sh
export RELEASE_REGISTRY='https://<approved-registry>'
export RELEASE_TAG='<approved-dist-tag>'
export RELEASE_ACCESS='<public-or-restricted>'
npm login --registry "$RELEASE_REGISTRY"
npm whoami --registry "$RELEASE_REGISTRY"
```

For **each** archive from one successful receipt, in the receipt's order:

```sh
# First review locally; this must not publish.
npm publish /absolute/path/to/verified-package.tgz --dry-run --ignore-scripts \
  --registry "$RELEASE_REGISTRY" --tag "$RELEASE_TAG" --access "$RELEASE_ACCESS"

# Separate, explicitly authorized publication command.
npm publish /absolute/path/to/verified-package.tgz --ignore-scripts \
  --registry "$RELEASE_REGISTRY" --tag "$RELEASE_TAG" --access "$RELEASE_ACCESS"
```

Use the exact reviewed archive, not the source directory. Verify its SHA-256
against `verified.json` immediately before publishing, and inspect existing
remote versions with `npm view <name>@<version> --registry "$RELEASE_REGISTRY"`.
Do not overwrite an existing version. `npm publish --dry-run` is supplemental
npm packaging feedback, not proof of authorization or registry acceptance.

There is no atomic multi-package npm release. Stop at the first failure, record
which exact versions succeeded, and inspect the registry before deciding what
to retry. Do not blindly rerun the whole list or automatically unpublish.
Publish providers before consumers; all packages in the receipt are preparation
candidates, not a mandate to republish unchanged versions. If publishing only
a subset, first ensure its dependencies and required peers already exist at
compatible versions in the approved registry.

Git commits, tags, pushes and remote releases remain separate authorized steps.
