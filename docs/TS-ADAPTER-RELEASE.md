# TypeScript adapter registry release candidate

This is local release preparation from main `5372c5de08bf4d7991beca8414c5885035bcad75`.
Nothing here authorizes npm/crates publication, a new Git tag, a push, or a main
merge. The existing GitHub CLI v0.2.0 release remains unchanged.

## Minimum npm set

Publish in this dependency order only after a separate approval:

| Package         | Registry latest at preparation | Candidate | Reason                                                                                                                        |
| --------------- | ------------------------------ | --------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `@lenso/engine` | 0.6.0                          | 0.7.0     | Current npm archive lacks `createWorkspaceSupervisor`; the new adapter imports it. Includes the existing workspace changeset. |
| `@lenso/cli`    | 0.20.1                         | 0.21.0    | Adds `./adapter`, preserves operation APIs, removes the standalone npm bin, and requires Engine `^0.7.0` in its real archive. |

`@lenso/core@0.3.1` is already published. Its downloaded, integrity-verified npm
archive has the same `src` as main, so it needs no new release for this bridge.
Other runtime dependencies remain ordinary registry versions. Neither the Rust
CLI nor the Runtime kernel/SDK is part of this npm publication set.

CLI 0.21.0 removes npm's executable. Install the already released Rust `lenso`
v0.2.0 separately and keep Bun available; ordinary TypeScript users need no Rust
compiler. Existing imports from `@lenso/cli` remain supported. Old `@lenso/mcp`
and other optional integrations may still require older Engine/CLI ranges;
upgrading or publishing those packages is outside this minimal pair's acceptance.
The two shipped Engine extension examples accept `^0.6.0 || ^0.7.0`.

## Two-package version preparation

The selected existing changesets are `workspace-service-development`,
`single-lenso-command`, and `tidy-example-engine-peers`. The official Changesets
APIs calculate the release plan, versions and changelogs. The repository wrapper
now supports an explicit package scope:

```sh
bun run release:status --changeset workspace-service-development \
  --changeset single-lenso-command --changeset tidy-example-engine-peers \
  --package @lenso/engine --package @lenso/cli
# On the unchanged pre-version source, use release:version with the same options.
```

Only Engine and CLI versions/changelogs advance. The scope must include every
explicit target in the selected changesets; unknown/private packages, missing
changeset selection, and splitting linked/fixed groups are rejected. Preview
writes nothing. Implicit dependent releases are retained in a pending changeset
before applying the scoped plan, rather than silently consuming their intent.
The unscoped Changesets path continues to handle all pending releases normally.

Seven optional Engine peers (api-keys, audit, authorization, notifications,
payments, runtime and webhooks) accept `^0.6.0 || ^0.7.0` in source to keep the
workspace consistent. These manifest-only compatibility changes have their own
pending patch changeset; they do not advance seven versions. Manage, MCP and
Scheduler dependency updates remain in a separate generated pending changeset.
Their versions and changelogs stay unchanged. Runtime authoring changesets remain
pending too. No third-party dependency resolution changes are required.

Already published optional integrations keep their old registry manifests until
a separate release; this candidate proves the whole source workspace and the
minimum installed Engine/CLI consumer, not a new registry release of all optional
integrations. The existing automatic Version PR #44 is broader and is not a
substitute. An ordinary candidate branch can later land without automatic
publication, followed by an explicitly approved manual `release.yml` dispatch
with only `@lenso/engine,@lenso/cli`. Neither action is authorized by this preparation.

## Clean installed consumer proof

Use Bun 1.4.3. Build and inspect real archives using the existing verifier:

```sh
bun run prepare:runtime
bun install --frozen-lockfile
bun run release:verify
bun scripts/verify-adapter-consumer.ts --verified /absolute/path/to/verified.json \
  --cli /absolute/path/to/verified/lenso --output output/release/adapter-consumer.json
```

The consumer script uses a read-only loopback staging registry, with candidate
packuments, real verified tarballs and SHA-512 integrity. Its manifest uses normal
package names and versions, with no `file:`, `link:`, `workspace:`, overrides or
source conditions. Candidate archives are actually fetched over HTTP into a new
Bun cache; unchanged Core and third-party dependencies come from public npm.
This is registry-shaped candidate validation, **not** a claim that the two
candidate versions are already available on npm.

A separate clean consumer installs actual current npm CLI 0.20.1 and demonstrates
both missing adapter export and missing Engine workspace supervisor. The positive
consumer checks frozen installation, dist resolution, side-effect-free adapter
import, declarations/typecheck, no npm bin, inspect without setup, call and
cleanup, check/generate/build, HTTP readiness, content-changing watch restart,
and owned listener shutdown. It runs the actual released Rust CLI v0.2.0 with
no Rust compiler or old CLI on PATH. Full acceptance currently runs on macOS arm64.

## Approval and remote requirements

Before publishing, approve the exact reviewed source commit and archive hashes,
registry `https://registry.npmjs.org`, access `public`, dist-tag `latest`, and only
these names/versions in order: `@lenso/engine@0.7.0`, `@lenso/cli@0.21.0`.
No other package version, Rust CLI release, Runtime release or Git tag is part of
that approval. Do not dispatch or merge the broad version PR under a two-package
approval.

Read-only inspection found the expected RELEASE_REGISTRY/ACCESS/TAG variables,
and an `npm-release` Environment restricted to main with LioRael as its sole
required reviewer. `prevent_self_review` is false and administrator bypass is
allowed. These settings differ from [the written release policy](RELEASING.md#remote-setup-required-before-enabling-publication).
The workflow references the Environment, and the local guard checks main/event/
job/OIDC markers; neither reads or rejects those two settings. GitHub enforces
its actual configured rules. This is a policy discrepancy, not a demonstrated
technical inability to run the current workflow.

Do not automatically tighten the Environment: preventing self-review when the
sole reviewer also starts the release would leave no approver. The smallest
owner decision is whether to explicitly permit the existing single-maintainer
approval arrangement for this pair (without using administrator bypass), with
a documented policy exception/adjustment. A distinct reviewer or initiator is a
separate policy choice if stricter separation is desired; it is not configured
by this candidate. This task changes no Environment or credentials.

`npm trust list` returned 401 without authenticated owner access. That does not
mean a Publisher is missing. Published Engine 0.6.0 and CLI 0.20.1 metadata and
provenance show historical GitHub Actions trusted publication from this same
repository, `release.yml`, main, and `npm-release`. Current trust settings still
need an owner-authenticated read-only check using existing access. Reuse a
matching Publisher; no new token, trust, or persistent permission is requested.
The OIDC publisher refuses token fallback, absent upstream dependencies, changed
archives, and overwrites of existing versions.

After a future approved publication, read back both registry versions and their
archive integrity, then repeat the clean consumer against real npm rather than
the staging registry. Passing this preparation does not replace that final gate.
