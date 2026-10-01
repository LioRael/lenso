# Configurable Engine Web / Contracts implementation checkpoint

Continuation completed final local acceptance: Relay 13 suites / 21 tests and
TypeScript passed; default/custom Web policies passed real HTTP requests and
clean shutdown. Candidate MCP inspection was performed and its custom Host
authority gap verified. See the current
[`release review`](engine-authoring-release-review-2026-10-01.md) for the
version/cohort proposal, first-publication blocker and final candidate checks.
The initial model-switch checkpoint below records the preceding stage.

## Ownership and state

Worktree: `feat/multi-plugin-source`, base
`119b9af70b82816588c00c8bf812f7c62d1a18b5`.
The coordinating task transferred write ownership after the multi-Plugin worker
stopped. Its existing uncommitted implementation and acceptance records remain
in place. The consumer overlay is `lenso-ai-relay/vendor/core`; it records exact
file digests in Relay `sources.json`. The implementation is preserved as a local
review snapshot. No push, CI dispatch, landing, publication or deployment has
been performed by this task.

The initial model-switch checkpoint preceded final consumer acceptance.
The implementation continues in this same worktree; do not reset the combined
diff or create a second implementation of the multi-Plugin changes.

## Implemented APIs and defaults

- Optional `lenso-engine-web`: `WebOptions`, `WebAuthoring`, `compile`, bounded
  `read_sources`, build helper and public `RouteSet::validate`. Defaults select
  `src/routes/**/*.rs`; explicit entries replace filename discovery. Provider,
  roots, exclusions, output and registration policy are configurable. Multiple
  handlers per file lower through the existing official Endpoint macro. Route
  IDs, methods, parameter shapes and matcher conflicts remain validated.
- Optional `lenso-engine-contracts`: `ContractAuthoring::from_inputs`, immutable
  `ContractInput`, explicit language/output/module projections, `discover`,
  `snapshot_contract`, `run`, `Mode::{Generate,Check}` and accepted baselines.
  Defaults select contract descriptors and TypeScript; configuration or custom
  Snapshot/input providers replace discovery. Cache and outputs are consumer
  local. Snapshot reads include only descriptors and required schema references.
- CLI default baseline policy uses the existing Engine atomic publication under
  `.lenso/contracts/accepted`. Unchanged descriptor/schema closures pass; changed
  ones require official compatibility/version validation. Check is read-only.
  `baseline_root` can be replaced or disabled for explicit baseline policies.
- Existing conventions now allow explicit support selection, owner scoping and
  bounded support-specific options. There is no second Plugin system or generic
  language/directory DSL. Adopted Root policy still gates compiler selection.
- App synchronization accepts published descriptor/schema snapshots from locked,
  materialized registry/git dependency closures. Source-only external inputs
  fail with a preparation prerequisite. Scanning never executes their exporters
  or build scripts. Outputs are consumer-local, prebuilt matching projections
  retain timestamps, and App check validates recorded freshness.
- Generator version plus source fingerprint participate in cache/freshness
  identity, including unpublished generator changes.

Architecture and configuration examples:
[`engine-web-contracts.md`](../architecture/engine-web-contracts.md).
Copyable native example: `examples/engine-web`.

## Relay integration

`lenso.contracts.json` selects the three existing projections and module names.
`tests/generate-contracts.sh` calls the optional official generator offline;
`build.rs` checks projection freshness and compiles Gateway routes with custom
roots and standalone registration. Fourteen buffered endpoints use generated
bindings and forward their original request/context to the existing HTTP code.
Grouped streaming registration, service lifecycle and concurrency remain in
the existing owner. CLI/Agent conventions were not added.

Generation selected 29 contracts. All 87 checked-in Rust, runtime Rust and
TypeScript projection files remained byte-identical. Relay offline Cargo check
passes against the synchronized overlay. Final consumer acceptance then passed
13 suites / 21 tests plus TypeScript. The previous multi-Plugin acceptance
record and packaged artifacts were not replaced by this task.

## Verification completed

- Contracts: 5 integration tests passed, including immutable dependency inputs,
  cache recovery, custom discovery, ownership guards, accepted-baseline equality
  and breaking-change rejection without baseline advancement.
- Web: 4 integration tests passed, including explicit Snapshot selection,
  multiple handlers, exclusions, invalid/duplicate paths and symlink rejection.
- Native example: official Endpoint macro compilation/routing inventory test
  passed. Subsequent default/custom HTTP requests and shutdown passed.
- App Contracts: 6 focused tests passed; one preexisting source-extraction test
  remains ignored because it requires registry access. Materialized registry/git
  fixtures verify consumer-local generation and unchanged dependency inputs;
  this is not a live registry/git acquisition test.
- Explicit convention/custom filename/options regression passed.
- Clippy with warnings denied passed for both new crates (all targets), and
  affected App/authoring libraries. Contracts was rechecked after the final fix.
- Relay `cargo check --locked --offline` passed after final overlay synchronization.

Machine-readable supporting evidence:
[`engine-web-contracts-2026-10-01.json`](engine-web-contracts-2026-10-01.json).

## Initial continuation plan (now superseded by release review)

1. Run the final Relay consumer acceptance: the existing 13 suites / 21 tests,
   TypeScript check and local packaged service acceptance. Use only existing
   authorized local/mock test infrastructure. Preserve previous receipts and
   packages; capture new receipts under a separate authoring acceptance directory.
2. Capture actual HTTP Health and typed path-parameter requests from the native
   Web example, with clean shutdown.
3. Build/use the candidate CLI for the approved read-only Relay MCP facts/check
   checkpoint. Existing CLI 0.6.6 evidence reports App resolution failure. Relay
   uses a custom Host; determine the candidate's actual authority/result rather
   than claiming successful composition without persisted built-App evidence.
   Do not alter the separately owned `.codex/config.toml` or pluginworker files.
4. Prepare the combined release set with dependency ordering. The two new
   support crates must exist before scaffold/App releases referencing them.
   Account for public `Compilation.options` and prior multi-Plugin public API
   additions when choosing pre-1.0 version increments. Regenerate/check consumer
   projections after generator version changes because generated headers change.
5. Parent coordinates Delta review, final-candidate SHA CI and exact-SHA landing,
   then separately authorized release publication and registry readback. Do not
   reuse CI for an amended candidate or changed base, and do not rerun the same
   full suite automatically after landing the identical SHA. No new PR delivery.

Toolchain PATH must include the real Rust 1.98.1 directory before mise shims.
Use `/tmp/lenso-authoring-target` or Relay `vendor/core/target`; the worktree's
target symlink points outside writable roots. Fresh quota lookup succeeded during
continuation: 13% remained. No task-owned test subprocesses remain active.
