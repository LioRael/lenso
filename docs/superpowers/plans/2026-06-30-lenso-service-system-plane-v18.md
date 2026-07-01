# Lenso Service System Plane V18 Plan

## Goal

Make Lenso understand a whole product system, not only one module release or one
service provider at a time.

V18 introduces `lenso.system.v1` as the graph above existing service and module
contracts. The graph makes service ownership, module capabilities, cross-service
dependencies, environment lanes, and operator issues visible to CLI tooling,
the host admin API, Runtime Console, examples, and docs.

## Scope

- Add a public `lenso.system.v1` schema and Rust data model in `lenso-service`.
- Add graph construction and validation for service/module ownership and
  capability dependency resolution.
- Add CLI commands under `lenso system` for init, service/module upserts, graph
  inspection, and plan checks.
- Expose `GET /admin/data/service-system` from the host admin data API.
- Surface the system plane on Runtime Console Services.
- Add a real examples manifest based on the support platform examples.
- Document the service system plane as Kubernetes-optional, not
  Kubernetes-required.

## Non-Goals

- Do not replace `lenso module install`.
- Do not replace `lenso service install`.
- Do not add service discovery, mesh policy, API gateway behavior, or
  distributed transactions.
- Do not make services peer runtimes. The host still owns auth, runtime queues,
  retries, outbox claims, Runtime Story, and Technical Operations.

## Verification

- `cargo test -p lenso-service`
- `cargo test -p lenso-platform-admin-data`
- `cargo test --locked system` in `lenso-cli`
- `lenso system plan --system-file ../lenso-examples/lenso.system.json --check`
- Runtime Console focused tests for service system fetching and Services page
  summaries.
- Site/docs build or type check when docs dependencies are available.
