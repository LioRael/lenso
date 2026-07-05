# Official Catalog Parity Design

## Goal

Keep the official module catalog surfaces aligned after first-party modules are
published.

The immediate gap is that `lenso` has an updated bundled official catalog entry
for `organization`, while `lenso-catalog-worker` still serves an older catalog.
The published `lenso-module-audit-log` crate also needs a deliberate catalog
decision instead of remaining an accidental omission.

## Current State

There are two official catalog publication surfaces:

- `crates/platform-admin-data/catalogs/lenso-official-module-catalog.json` in
  `lenso`. This is embedded into the host admin-data API and backs the Runtime
  Console available-modules view when no host-local discovery file or live remote
  registry is present.
- `catalogs/lenso-official-module-catalog.json` in `lenso-catalog-worker`. This
  is served from `https://catalog.lenso.dev/v1/modules.json` for CLI named module
  installs and external discovery.

This work must not restore the old project-local catalog maintenance workflow.
`lenso module install <name>` should continue resolving names from the official
remote catalog by default, with `--catalog-url` as the override. Commands such as
project-level `module catalog add` are outside this design.

## Scope

The parity slice updates official catalog data and validation only.

In scope:

- Sync the `organization` module entry from the `lenso` official catalog into
  `lenso-catalog-worker`.
- Decide whether `audit-log` should be listed now, using the published
  `lenso-module-audit-log` crate and current first-party module rules.
- Update worker tests so they assert important catalog entries rather than only
  preserving a stale module count.
- Audit local repo state for catalog-related unpushed, dirty, ahead, and behind
  changes before opening or merging PRs.
- Keep catalog JSON schemas and names compatible with the existing worker parser
  and host admin-data parser.

Out of scope:

- A dedicated `@lenso/audit-log-console` package.
- Reintroducing host-local catalog authoring or install-by-local-catalog flows.
- New module implementation work.
- Redesigning module catalog format versioning.

## Catalog Decisions

`organization` should be present in both official catalogs because it is a
published first-party linked module with a published console package:

- module: `organization`
- console package: `@lenso/organization-console@0.1.0`
- source: `linked`
- manifest reference: `builtin:organization`

`audit-log` should be added to the official catalogs as a linked first-party
module if the implementation plan verifies that the published crate is the
current install target and the host can treat `builtin:audit-log` consistently
with other linked first-party modules.

The first `audit-log` catalog entry should not advertise a console package.
The module already exposes a schema-admin read surface through its manifest, so
Runtime Console can show it through generic module data once installed.

## Data Flow

The remote named-install path remains:

```text
lenso module install <name>
  -> https://catalog.lenso.dev/v1/modules.json
  -> module descriptor
  -> install linked or service-backed module
```

The host available-modules fallback path remains:

```text
Runtime Console available modules
  -> /admin/data/available-modules
  -> host-local discovery file if present
  -> live remote registry when remote modules are loaded
  -> embedded official catalog snapshot
```

The two official snapshots should contain the same official modules unless a
future design explicitly introduces staged rollout metadata.

## Error Handling

The worker should continue returning:

- `catalog.not_found` for unknown module names.
- `catalog.internal_error` for malformed bundled catalog data.
- `no-store` cache headers for health and error responses.

If an official entry references an unreachable console bundle or unpublished
crate/package, the implementation should fail validation before deployment
rather than shipping a partially discoverable module.

## Testing

Catalog parity should be verified with:

- Worker unit tests for `/v1/modules.json`, `/catalog/modules.json`, `/healthz`,
  and `/v1/modules/:name`.
- Assertions that `organization` is served from the worker catalog with its
  console package metadata.
- Assertions for `audit-log` if it is added in this slice.
- Host-side catalog/admin-data tests only if the `lenso` embedded catalog
  changes in this implementation.
- Package registry checks for any newly referenced npm package, and crates.io
  checks for any newly referenced Rust crate.

## Rollout

1. Recheck repo state in `lenso`, `lenso-catalog-worker`, `lenso-runtime-console`,
   and `lenso-organization-module`.
2. Update only the official catalog files needed for parity.
3. Run targeted catalog tests.
4. Open and merge PRs per repository, keeping unrelated dirty local changes out
   of the staged set.
5. Deploy `lenso-catalog-worker` only after the merged catalog data passes tests.
6. Verify the live catalog endpoint serves the expected module set. If direct
   `curl` receives environment-specific 403 responses, use worker-local tests and
   the available deployment verification surface before declaring the remote
   rollout complete.

## Success Criteria

- The official remote catalog and host embedded fallback agree on the official
  module set for this slice.
- `organization` is discoverable from the worker catalog with its console
  package metadata.
- `audit-log` is either added with a validated linked catalog entry or explicitly
  deferred with a reason recorded in the implementation plan.
- No project-local catalog maintenance workflow is restored.
- Tests cover the entries most likely to drift.
