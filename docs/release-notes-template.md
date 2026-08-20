# Lenso <version>

> **Legacy v0.3.x maintenance guide:** This page applies to the maintained
> Service-oriented release line on `main`. It does not define vNext
> architecture or authorize publication from `next`. See the
> [documentation map](README.md).

## Summary

<One short paragraph describing why this release matters.>

## Release Inputs

- Commit: `<sha>`
- Gate: explicit CI quality commands and generated-artifact check result
- Console: sibling repository check result, if coordinated
- Generated artifacts: owner-local generator and generated-artifact test result

## First Release Scope

- Linked modules load through the app bootstrap composition root.
- Services install through `lenso service install <service-name-or-manifest>` and
  provide modules to the host.
- Service-provided modules can declare schema-admin, HTTP routes, runtime
  functions, and lifecycle activation jobs.
- Console integration is provided by the separate
  `lenso-console` repository.
- Generated contracts are committed and reproducible.

## Getting Started

```sh
docker compose -f infrastructure/local/docker-compose.yml up -d postgres
cargo run --locked -p lenso-migrate
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo test --locked -p lenso-api-contracts --test architecture
```

## Known Caveats

- Local service verification requires Postgres and separate API, worker, and Console
  shells.
- Service install is manifest-based and low-friction.
- Publisher trust, registry review, install history, doctor flows, bundle
  import/export, provenance, and signatures are not release blockers.
