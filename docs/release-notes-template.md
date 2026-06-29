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
- Services install through `lenso service install <service-name-or-manifest>` and
  provide modules to the host.
- Service-provided modules can declare schema-admin, HTTP routes, runtime
  functions, and lifecycle activation jobs.
- Runtime Console integration is provided by the separate
  `lenso-runtime-console` repository.
- Generated contracts are committed and reproducible.

## Getting Started

```sh
just db-up
just migrate
just check
```

## Known Caveats

- Local service smoke requires Postgres and separate API, worker, and Console
  shells.
- Service install is manifest-based and low-friction.
- Publisher trust, registry review, install history, doctor flows, bundle
  import/export, provenance, and signatures are not release blockers.
