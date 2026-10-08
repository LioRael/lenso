# Release preparation and protected CI publication

Local preparation never publishes, pushes, creates Git tags or remote releases.
The GitHub Actions release workflow can publish only through its separate
`npm-release` Environment job after repository administrators configure approval.
Use the repository's Bun version (`1.4.2`) and installed dependencies. `tar` must
be available on PATH. Do not invoke a real local publish as a validation step.

## Decide before a first publication

The repository does not establish a registry, ownership of the unscoped names
`lenso` and `lenso-cli` or the `@lenso` scope, package access, license, or dist-tag.
Confirm those with the release owner. Do not infer a registry from a package
name. The ten public package manifests declare MIT and the GitHub repository
with their package directories. Local checks cannot prove name availability or
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
policy. CI publication uses its own mandatory explicit policy, not that fallback.
Do not use prerelease or snapshot mode without an explicit release policy.
The CI receipt validator refuses prereleases under `latest`.

## Validate locally

From the repository root:

```sh
bun install --frozen-lockfile
bun run release:test
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

## Three GitHub Actions workflows

1. `checks.yml` runs for ordinary PRs, pushes to `main`, and manual dispatch.
   Its only token permission is `contents: read`; checkout does not persist
   credentials. Fork code never receives repository write or OIDC permission.
   It performs frozen install, lint, formatting check, build, typecheck and tests.
2. `version.yml` runs on `main`, using the pinned Changesets action with only a
   `version` command. It updates manifests/changelogs/consumed changesets and the
   one root `bun.lock`, checks frozen installation, formats, and opens/updates a
   version PR. There is no publish input, npm credential or OIDC permission.
3. `release.yml` is `workflow_dispatch` only, restricted to `main`. Its prepare
   job repeats checks and runs the existing archive verifier, then writes a
   `release.json` receipt for the explicitly requested comma-separated package
   names. The immutable artifact contains the same verified tarballs and receipts.
   An independent Environment job downloads that artifact and invokes the
   explicit `publish <release.json>` command. It performs no install of workspace
   dependencies, package build, pack, Changesets publish, tag or remote release.

The prepare job verifies all public framework packages to preserve build
dependency ordering; only the explicit receipt subset is published. `release.json`
binds source SHA, repository, workflow run ID, preparation attempt, policy, ordered
names/versions, archive basenames, hashes and file lists. It is not a Changesets
pack manifest. Preparation checks that the actual checkout matches the source
SHA and remains unchanged before and after building. The receipt's selected set
must match the dispatch inputs. Archive hashes and identities are checked for the whole batch
before writes and hashes are checked again immediately before each publish.
Runtime/optional/peer dependencies in the selected set must precede their
consumers and satisfy packed ranges; omitted dependencies must have compatible
versions visible in the approved registry. Registry failures stop the release.
Optional dependencies/peers are conservatively required by this CI policy.

### Version PR checks and `GITHUB_TOKEN`

The version action uses the ephemeral `GITHUB_TOKEN`, not a long-lived PAT.
GitHub suppresses workflows triggered by most events this token creates, so the
bot's version PR and updates do **not** automatically trigger ordinary PR checks.
Do not merge it assuming checks ran. The recommended no-token workaround is:

1. Fetch/review the bot branch, make a deliberate human-authored commit (an empty
   commit is sufficient) and push it with your normal human Git authentication.
2. Confirm the required `Checks / checks` check runs for the new PR head SHA.
3. If the bot updates that branch again, repeat before merging.

Alternatively, manually dispatch `checks.yml` on the bot branch and review that
exact SHA. A dispatch run is not promised to satisfy a PR-required status rule;
verify your branch protection behavior. If fully automatic bot PR checks become
necessary, an explicitly approved short-lived GitHub App installation token is
a follow-up, not a hardcoded PAT or hidden prerequisite here. See GitHub's
[triggering workflows](https://docs.github.com/en/actions/how-tos/writing-workflows/choosing-when-your-workflow-runs/triggering-a-workflow).

### PostgreSQL test isolation

`scripts/ci-checks.sh` requires installed `postgres`, `initdb`, `pg_ctl` and
`createdb`. On Ubuntu 24.04 CI installs PostgreSQL 16 binaries and adds their bin
directory to PATH. The script uses a unique temporary data directory and an
ephemeral loopback TCP port, no shared PostgreSQL service or fixed port. The
allocation-to-start interval has the same small port race as existing Auth tests;
a collision fails startup, never connects to a shared database. EXIT cleanup
stops only the cluster identified by that data directory.

Root `bun run test` runs ordinary tests, plus Auth's independently owned temporary
clusters with `LENSO_REQUIRE_POSTGRES=1`. `turbo.json` explicitly forwards that
switch in strict environment mode. The URL-dependent PG tests are then run
serially with `LENSO_TEST_DATABASE_URL` for Notes and `TASK_TEST_DATABASE_URL` for
Tasks. Each of Notes, Tasks package, and Tasks example has a different fresh
database. The Tasks example's `authorization-test` queue is explicitly migrated
before its PG/entry tests; it is not shared with package tests or another CI job.
Their UUID data can remain until the owned temporary cluster is destroyed.
Ambient DB URLs are unset so local reproduction cannot reuse a production URL.

### Remote setup required before enabling publication

These steps are administrator work; adding YAML does not configure remote
approval or package ownership:

1. Confirm name/scope ownership, license/legal metadata, exact release set,
   registry, access and tag. This implementation supports npm's OIDC registry
   only. Selecting a different registry fails closed; adapting to another
   registry's verified short-lived authentication is a separate design decision.
   Anonymous registry verification cannot read restricted/private packages, so
   those fail closed rather than gaining a fallback read token.
   Set each selected source manifest's `repository.url` to the actual GitHub
   repository, as npm requires. The public manifests declare repository metadata
   and MIT, confirmed by the release owner. The publisher refuses
   missing/mismatched repository URLs before registry writes.
2. Configure repository variables `RELEASE_REGISTRY`, `RELEASE_ACCESS`,
   `RELEASE_TAG`, or supply explicit dispatch inputs. No value has a default;
   blank/malformed policy fails. The supported registry value, if approved, is
   exactly `https://registry.npmjs.org`; access is `public` or `restricted`.
3. Create the GitHub Environment **`npm-release`**, require designated reviewers,
   prevent self-review, restrict deployment branches to protected `main`, and
   prohibit protection bypass where your GitHub plan permits it. Protect `main`
   and require review of workflows/release scripts. Without remote protection,
   `environment:` alone is not an approval gate. Do not dispatch until configured.
4. For every package, configure npm Trusted Publisher for this exact GitHub
   organization/user, repository, workflow filename **`release.yml`**, and
   Environment **`npm-release`**. Allow direct `npm publish` in npm's current
   trusted-publisher settings. New package names may require an owner-authorized
   first-publication/bootstrap procedure before npm exposes package settings;
   this repository does not automate bootstrap or store a fallback publish token.
   npm's current documentation says a new publisher configuration must complete
   its first successful publication within two days, otherwise recreate it.
5. Enable Actions PR creation for the version job and allow its scoped
   contents/PR-write permissions. Configure required checks, including the bot
   PR procedure above. Check organization/fork approval restrictions too.
6. Dispatch release on `main` with explicit package names. Before approving the
   publish job, download/review `release.json`, source SHA, policy, complete
   archive file lists/hashes and all preparation checks. Artifacts expire in
   14 days. Do not approve stale or unexplained batches.

The local script guard refuses publication without dispatch/main/protected-job
markers and OIDC environment, and refuses common token fallback variables.
These markers are not cryptographic proof of an Environment approval: the real
enforcement is GitHub protection plus npm's exact trusted publisher identity.
There is intentionally no root `release:publish` shortcut.

### Toolchain and official references

Bun is pinned to project `1.4.2`. Node `24.21.0` is pinned from the official
[Node distribution index](https://nodejs.org/dist/index.json), on the supported
24 LTS line. npm `12.2.0` is pinned from official
[npm package metadata](https://registry.npmjs.org/npm/12.2.0); its Node engine
accepts `^24.15.0`. npm's current
[Trusted Publishers documentation](https://docs.npmjs.com/trusted-publishers/)
requires npm >=11.5.1 and Node >=22.14.0 and supports GitHub-hosted runners,
not self-hosted runners. OIDC permission exists only on the publish job.
The npm CLI is installed globally there, with an empty temporary user config;
there is no registry-url setup-node credential template or repository npm token.
Trusted publishing automatically generates provenance where npm supports it;
private-package provenance and first-publish acceptance are not locally proven.

Every `uses:` is pinned to a full commit SHA resolved from official repository
tags via `git ls-remote`: checkout v4.3.1, setup-node v4.4.0,
upload-artifact v4.6.2, download-artifact v4.3.0, changesets/action v1.5.3,
and oven-sh/setup-bun v2. Do not replace pins with moving major tags.

### Partial publication and retry

npm releases are not atomic. Publishing stops on the first error, with no
automatic rollback, unpublish, version overwrite, or dist-tag mutation on retry.
Before any write, existing selected versions are downloaded from the registry
and their archive SHA-256 must match the receipt, not merely their version
number. Identical archives are safely skipped; differing bytes or unreadable
registry content fail closed. After each publish the registry archive is checked.
A visibility delay can stop the run even after a successful write.

After inspecting logs and registry state, rerun only the failed publish job of
the same workflow run to reuse the original prepare job's artifact output.
The receipt permits a later attempt of that same source/run, not another run.
GitHub rerun output propagation and Environment reapproval must be verified
remotely. Rerunning all jobs rebuilds a new batch and may create different archive
bytes; those will not be silently accepted for already published versions.
If the original artifact expired, recover the exact reviewed archive through
an owner-approved process; do not substitute a repack and call it the same batch.

Local tests use registry mocks and never publish. They cannot prove GitHub
Environment protection, fork token restrictions, hosted-runner provisioning,
OIDC exchange, npm permissions/name availability or real Actions execution.
Git commits, tags, pushes, deployment and remote releases remain separate
authorized steps.
