# Lenso <version>

## Summary

<One short paragraph describing why this release matters.>

## Release Inputs

- Commit: `<sha>`
- Gate: `just release-check` result
- Runtime Console: sibling repository check result, if coordinated
- Generated artifacts: `just generated-check` result

## First Release Scope

- Linked modules load through the app bootstrap composition root.
- Remote modules install through `lenso module add <manifest-url>`.
- Remote module manifests can declare schema-admin, HTTP routes, runtime
  functions, and lifecycle activation jobs.
- Runtime Console integration is provided by the separate
  `lenso-runtime-console` repository.
- Generated contracts and the TypeScript SDK are committed and reproducible.

## Getting Started

```sh
just install
just db-up
just migrate
just check
```

## Known Caveats

- Local service smoke requires Postgres and separate API, worker, and Console
  shells.
- Remote module install is decentralized and low-friction.
- Publisher trust, registry review, install history, doctor flows, bundle
  import/export, provenance, and signatures are not release blockers.
