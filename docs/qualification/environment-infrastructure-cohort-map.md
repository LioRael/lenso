# Environment and infrastructure delivery cohorts

This is the risk map for the environment-and-infrastructure program. It is a
test-selection policy, not a status board: current implementation, release,
and qualification assertions remain solely in the
[qualification ledger](qualification-status.json).

Each row becomes one exact cohort only after every owner has assembled its
same-repository changes into a source closure. Parallel worktrees are useful
for development but are never silently treated as one dependency closure.

| Changed contract | Source owners | First artifact consumers | Required focused cohort evidence |
| --- | --- | --- | --- |
| Execution-target capability profile vocabulary and fail-closed selection | `lenso`, `lenso-runtime-rust`, `lenso-protocols`, `lenso-bun-adapter` | `lenso-cli`, `lenso-engine` | Rust and TypeScript protocol fixtures, runtime/Bun profile conformance, and CLI explain/preflight against exact profile artifacts. |
| Deterministic TestApp simulator controls | `lenso-runtime-rust` | Runtime-owned simulator tests and an Auth scenario consumer | archive/extracted Rust consumer, deterministic receipt check, and selected Auth consume/revoke/restart cases. |
| Workers host callback, stream, and target harness | `lenso-runtime-rust`, `lenso-web` | local `workerd` Host composition | real local `workerd` ingress plus callback failure/cancellation, artifact digest for Wasm/bundle inputs, startup and clean shutdown receipt. |
| OAuth private Postgres adapter and migration semantics | `lenso-auth-plugin` | Native PostgreSQL reference Host and Workers callback Host | extracted `.crate` closure, Native PostgreSQL gate when its resource is available, local `workerd` callback composition, and explicit migration upgrade route. |
| Opt-in OpenAPI contract and simulated Web Host | `lenso-web`, `lenso-engine` | generated Web Plugin and real ingress host | packed Web crates, generated Plugin compile, OpenAPI drift test, simulated request test, TCP startup/shutdown smoke. |
| Console / Agent state evidence | `lenso-console`, `lenso-agent` | their independent Hosts | owner package startup/shutdown and the specific state/outcome fixture; these do not widen a runtime target claim. |

## Cohort construction order

1. Freeze one assembled candidate per owner repository. Capture its full SHA
   and `lenso.git-worktree-snapshot-v1` source digest. A dirty candidate stays
   `candidate`; it is never release-ready or published.
2. Build only the artifacts the affected consumers load: Rust `.crate`
   archives, npm tarballs, Wasm modules, bundles, or executable packages. Add
   byte sizes and SHA-256 digests to the cohort.
3. Install into a temporary consumer directory from those artifacts. Source
   paths and pre-existing sibling checkouts are forbidden for the dependency
   closure under test.
4. Run the smallest real Host scenario that reaches startup and shutdown. For
   a stateful owner, run its explicit previous-to-new upgrade/migration route.
5. Retain each command receipt with the cohort. A committed closure with the
   first six local stages passed is `release-ready` only when its `upgrade`
   stage also passes, or when a genuinely stateless package records
   `upgrade: not-applicable` with an explicit reason and limitation. Stateful
   compositions, including Auth storage, must pass their migration route.
   `release-ready` is not published. Record `published` only after every
   artifact has an externally verifiable publication URL and retained receipt;
   then attach the matching publication receipt to any ledger `Released`
   facet. Rebuild and recapture after any candidate amendment.

## Boundary rules

- Cargo patching is allowed only to extracted artifacts in the clean room; an
  in-place workspace patch is a development check, not package-install proof.
- An npm tarball installation must use a fresh package-manager cache or an
  isolated prefix when cache identity could mask the artifact under test.
- Startup and shutdown are Host lifecycle facts. A standalone library unit
  test, Wasm compile, or constructor call cannot substitute for them.
- Auth migration upgrade evidence must name source/target schema versions and
  the expected preserved or explicitly rejected data state.
- The real Cloudflare target, external infrastructure, and production
  qualification remain separate steps after a local artifact cohort passes.
