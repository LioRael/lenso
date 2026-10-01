# Engine Contracts support

Optional official Capability projections through `lenso_engine::Plugin`.
Use `discover` with `DiscoveryOptions`, or provide your own immutable Snapshot
and `ContractInput` selections to `ContractAuthoring::from_inputs`.

`run` supports generation, read-only freshness checking, scoped Rust modules,
compatibility baselines and consumer-owned content-addressed cache/output paths.
It validates every selected contract before installing projections and never
executes dependency-provided code. External package acquisition and locking belong
to the caller; already materialized descriptor/schema inputs work offline.

The optional binary uses `lenso.contracts.json`:

```sh
lenso-engine-contracts generate .
lenso-engine-contracts check .
```

Defaults select `contracts/**/capability.json` and TypeScript. Explicit targets,
modules, roots, descriptor filename and exclusions replace those defaults.
The CLI accepts successful generations into `.lenso/contracts/accepted` using
Engine atomic publication; later changes require official compatibility checks.
Set `baseline_root` to another path or `null` for an explicit baseline policy.
See the workspace's `docs/architecture/engine-web-contracts.md` for the complete
API and App dependency integration boundary.
