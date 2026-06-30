# Lenso System Release Train V20 Plan

## Goal

Make a whole Lenso system releasable as one reviewed change set.

## Scope

- Add `lenso.system-release.v1` plans in the CLI.
- Add `lenso system release plan/check/apply/promote/rollback/history`.
- Store applied system releases in `.lenso/system-releases.json`.
- Keep service release plans, module installs, and cluster deployment explicit.
- Expose `GET /admin/data/service-system/release-train`.
- Show the release train on Runtime Console Services.
- Add support-platform release fixtures.

## Non-Goals

- No automatic module install.
- No automatic service release apply.
- No Kubernetes or operator writes.
- No service mesh, distributed transaction, or gateway behavior.

## Verification

- `cargo test --locked system` in `lenso-cli`.
- `cargo test -p lenso-platform-admin-data` in `lenso`.
- `just generated-check` and `just arch-check` in `lenso`.
- Runtime Console focused tests and typecheck.
- Example `system release check` and `system release history` commands.
