# Repository Operations

This repository is the backend half of the Lenso repo pair. It owns the Rust
services, platform crates, modules, migrations, contracts, and admin APIs
consumed by the Console.

## Repository Pair

Keep the backend and Console checked out as siblings:

```text
framework/
  lenso/
  lenso-console/
```

- Backend: `LioRael/lenso`
- Lenso Console: `LioRael/lenso-console`

The Lenso Console repository owns the React/Vite frontend, deployable Console
Service backend, Console extensions, and service SDKs. This framework
repository owns public contracts, module manifests, and the compatible Host
admin APIs consumed by the Console.

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

That gate checks Rust formatting, compiles and tests the Rust workspace,
verifies generated contracts, and runs architecture checks.

## Console Compatibility

The Console releases independently from this repository. It consumes published
framework crates, npm packages, and committed contract artifacts rather than
checking out this repository in CI. When a contract or dependency changes,
update the Console consumer and run its focused compatibility checks; do not
restore a cross-repository checkout or a shared release channel.

## GitHub Repository Metadata

Current repository metadata should stay aligned with the README:

- Description: `Rust-first modular monolith backend with generated contracts and Console admin APIs`
- Topics: `axum`, `lenso`, `lenso-console`, `modular-monolith`, `openapi`, `postgres`, `rust`

Update GitHub metadata when the repository role changes materially.

## History Backup

The backend repository was reset to a clean single-commit baseline after the
Console split. The pre-squash history is preserved on:

```text
archive/pre-squash-history
```

Do not delete that branch unless the old history has been intentionally archived
somewhere else.

## Migration Checklist

When moving the Lenso repositories to a new owner or recreating one of them:

1. Push `lenso`, `lenso-console`, and `lenso-cli` with their repository-local
   release workflows and independent version streams.
2. Reapply `main` branch protection in each repository.
3. Verify the required check name is still `quality`.
4. Verify the Console consumer uses published framework contracts and no
   central release checkout or deploy key.
5. Run the main-branch CI workflows and confirm they pass.
6. Preserve or intentionally replace `archive/pre-squash-history`.
7. Update README repository links and GitHub metadata if owner or repo names changed.
