# Lenso <version>

## Summary

<One short paragraph describing why this release matters.>

## Release Inputs

- Commit: `<sha>`
- Gate: `just release-check` result
- Demo: `just demo-release` result
- Generated artifacts: `just generated-check` result

## First Release Scope

- Linked modules load through the app bootstrap composition root.
- Remote modules install through `lenso module add <manifest-url>`.
- Remote module manifests can declare schema-admin, HTTP routes, runtime
  functions, and lifecycle activation jobs.
- Runtime Console shows loaded modules, remote calls, runtime functions, and
  lifecycle activation declarations.
- Generated contracts and the TypeScript SDK are committed and reproducible.

## Getting Started

```sh
just install
just db-up
just migrate
just demo-release
```

## Known Caveats

- Local service smoke requires Postgres and separate API, worker, and Console
  shells.
- Remote module install is decentralized and low-friction.
- Publisher trust, registry review, install history, doctor flows, bundle
  import/export, provenance, and signatures are not release blockers.
