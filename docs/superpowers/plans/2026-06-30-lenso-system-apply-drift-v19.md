# Lenso System Apply And Drift V19 Plan

## Goal

Turn `lenso.system.json` from a readable graph into an operator workflow:
compare declared system state with host-local state, apply the safe local parts,
and show drift in Runtime Console.

## Scope

- Add `lenso system diff`, `lenso system apply`, and `lenso system doctor`.
- Let `system apply` write only `.lenso/module-services.json` and
  `.lenso/service-environments.json`.
- Keep module install, service release, and Kubernetes/operator deployment
  explicit commands.
- Add `GET /admin/data/service-system/drift`.
- Show drift and next command on Runtime Console Services.
- Add a ready host-local state fixture to `lenso-examples`.

## Non-Goals

- No automatic module install.
- No automatic release apply.
- No cluster writes.
- No service mesh, discovery, or repair loop.

## Verification

- `cargo test --locked system` in `lenso-cli`.
- `cargo test -p lenso-platform-admin-data` in `lenso`.
- `just generated-check` and `just arch-check` in `lenso`.
- Runtime Console focused tests and typecheck.
- `lenso system diff --system-file lenso.system.json --repo-root fixtures/system-state/ready --check` in examples.
