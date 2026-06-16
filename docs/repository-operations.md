# Repository Operations

This repository is the backend half of the Lenso repo pair. It owns the Rust
services, platform crates, modules, migrations, contracts, generated TypeScript
SDK, and admin APIs consumed by the Runtime Console.

## Repository Pair

Keep the backend and Runtime Console checked out as siblings:

```text
framework/
  lenso/
  lenso-runtime-console/
```

- Backend: `LioRael/lenso`
- Runtime Console: `LioRael/lenso-runtime-console`

The Runtime Console repository owns the React/Vite frontend. This backend
repository owns the `/admin/runtime/*`, `/admin/data/*`, module manifest, and
contract surfaces that the Console consumes.

## Branch Protection

Both repositories protect `main` with the same baseline:

- Changes must enter through pull requests.
- The required status check is `quality`.
- Status checks are strict, so branches must be up to date before merge.
- Linear history is required.
- Force pushes are disabled.
- Branch deletion is disabled.
- Required approval count is `0`.
- Admin enforcement is enabled so repository admins follow the same protection rules.

Use squash merges by default. Use rebase merges only when preserving multiple
commit boundaries matters. Standard merge commits are disabled.

## Continuous Integration

The backend `ci` workflow runs on pull requests and pushes to `main`.

The `quality` job runs:

```sh
just ci
```

That gate installs the optional SDK dependencies with a frozen lockfile, checks Rust
formatting, compiles and tests the Rust workspace, verifies generated contracts
and SDK files, runs architecture checks, and typechecks the TypeScript SDK.

The workflow uses Node 24 with Node 24-native GitHub Actions.

## Runtime Console CI Dependency

The Runtime Console CI checks out this backend repository to typecheck and build
against the backend admin API contracts and fixtures.

The cross-repository checkout is configured with:

- Backend deploy key: `lenso-runtime-console CI read key`
- Backend deploy key mode: read-only
- Runtime Console secret: `LENSO_REPO_DEPLOY_KEY`

If either repository is recreated, transferred, or renamed, recreate the
read-only deploy key on the backend repository and store the private key in the
Runtime Console repository secret with the same name.

## GitHub Repository Metadata

Current repository metadata should stay aligned with the README:

- Description: `Rust-first modular monolith backend with generated contracts and Runtime Console admin APIs`
- Topics: `axum`, `lenso`, `modular-monolith`, `openapi`, `postgres`, `runtime-console`, `rust`

Update GitHub metadata when the repository role changes materially.

## History Backup

The backend repository was reset to a clean single-commit baseline after the
Runtime Console split. The pre-squash history is preserved on:

```text
archive/pre-squash-history
```

Do not delete that branch unless the old history has been intentionally archived
somewhere else.

## Migration Checklist

When moving this repo pair to a new owner or recreating either repository:

1. Push both repositories and keep them as private repos unless intentionally publishing them.
2. Reapply `main` branch protection in both repositories.
3. Verify the required check name is still `quality`.
4. Recreate the Runtime Console read-only backend deploy key.
5. Recreate `LENSO_REPO_DEPLOY_KEY` in the Runtime Console repository.
6. Verify both CI workflows use Node 24-compatible action versions.
7. Run both main-branch CI workflows and confirm they pass.
8. Preserve or intentionally replace `archive/pre-squash-history`.
9. Update README repository links and GitHub metadata if owner or repo names changed.
