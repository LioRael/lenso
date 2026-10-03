# Source assembly coverage, 2026-10-03

This ledger distinguishes reused behavior, implemented slices, remaining proof
and explicit non-goals. ADR 0079 is the decision; examples and runtime checks are
implementation evidence. No row turns an architectural constraint into a claim
that a new feature has shipped.

| Section | Coverage and current evidence |
| --- | --- |
| 1 | Plugin remains the sole behavior unit; existing macros and both source examples are reused. |
| 2 | Progressive complexity: `source-first-instances` needs only source, Instance config and two named Dependencies. |
| 3 | Private Rust/TS modules remain ordinary modules; no new behavior category or mandatory Capability was introduced. |
| 4 | Source and normal dependencies use discovery plus the existing generated Host/resolver; Console/Auth are exact Git dependencies. |
| 5 | Local examples use ordinary `app build/dev`; no Plugin pack/freeze/install. Portable immutable artifacts retain their existing consumption boundary. |
| 6 | One Label implementation has independent two-Instance config/state/bindings in real Native and Workers requests. Native resource catalogs and Kernel binding/permission guards are retained; configuration-provider and multi-Console isolation evidence belongs to their active owner tasks. |
| 7 | Dedicated support-combination owner is implementing exact combination declaration/admission; not substituted by version strings. |
| 8 | Specialized PostgreSQL Auth remains specialized. This Console composition is Native only; no universal target/storage requirement. |
| 9 | Normal CLI discovery → convention selection → assembly → `load_resolved_app`/`resolve_runtime_app` is the common App authority. |
| 10 | Rust App uses the exact common resolved Plan in Native, Workers and DeterministicDriver/Simulated, with the same source implementations and shared Ingress routing. Mixed Rust/TS App has real Native/Workers proof; Bun deterministic simulation is unassessed. |
| 11 | Rust and TS source coexist in one project; direct Rust typed clients and the selected Bun/Workers JS adapters remain distinct execution mechanics. |
| 12 | Only genuine Capability calls cross Rust/TS. Right TS Instance calls left through the Kernel-selected dependency; no second JS resolver or mandatory bridge for private modules. |
| 13 | HTTP route authoring is real in all examples. Existing generated Request/Stream/Event Capability and Native adapter seams are retained; Core JS Workers currently rejects Stream/Event. SDK Stream adapter qualification is not full App qualification. |
| 14 | Existing generation-owned `ManagedTasks` and lifecycle hooks remain the lightweight background-work seam. A new cron convenience/generator and its composed-App proof have not been implemented in this slice; no Descriptor/Host authoring shortcut is claimed. |
| 15 | Relay endpoint/model/price configuration is owned by Relay/configuration tasks; no Core or Relay production resources changed here. |
| 16 | Kernel has no database concept. Original API-token Auth performs private PostgreSQL persistence and setup via its original operator; real local storage lifecycle tests pass. Existing semantic Store contracts are retained without forcing GenericDatabase. |
| 17 | Three-D1 consolidation is explicitly out of scope. |
| 18 | Official Console + original Auth + protected HTTP is one normal CLI App. Multi-Instance mount work is with the Console owner and not claimed by this single-Console demonstration. |
| 19 | Inherited dev-loop re-resolves config and reuses execution artifacts; integrated real HTTP probe confirms no build invocation and unchanged artifact hashes. |
| 20 | Rust edits relink the changed package plus generated Host; unrelated Health stays cached. No dynamic Rust replacement claim. |
| 21 | Target validation remains separate from common resolution. Mixed JS Stream/Event rejection now occurs before Native assembly; broader support-preflight task remains with its owner. |
| 22 | Agent DX candidate `5d217697` is queued for unique integration. Its skills/docs/real source-handler flow simplify existing commands without new MCP/runtime APIs; inherited dev feedback MCP remains. |
| 23 | The same candidate updates the official skill pack and validator; no new packaging vocabulary is exposed. |
| 24 | Before/after feedback samples are recorded in `docs/performance/dev-loop-20261003.json`; integration adds a separate Linux sample, with no CI extrapolation. |
| 25 | Rust and TS examples run through normal CLI and existing contract generation. Internal factory/codec/generation/artifact work is automatic. |
| 26 | Relay's initial Cloudflare-only production loop is independent and does not wait for Native qualification or this integration. |
| 27 | No Contribution/Managed Module hierarchy, universal storage abstraction, all-portable conversion, libbun embedding or Marketplace work. |
| 28 | Natural `plugins/<id>/plugin.rs` and `plugin.ts` directories plus adjacent configs. Console example consumes original Auth and its original operator, rather than copying Auth implementation. |
| 29 | ADR 0079 records the ten decision points below and their implementation boundaries. |

The ten decisions are: one Plugin behavior unit; progressive complexity;
source/dependency/portable origins; independent Instances; common composition
with target validation/lowering; specialized target/storage support; first-class
Rust/TS with real Capability boundaries; existing lightweight behavior authoring
and generated internals; risk-based verification and truthful feedback evidence;
and independent product delivery with explicit non-goals.

## Actual local composition checks

- Rust source App: real Native and workerd HTTP, clean exits and identical
  resolved graph. Simulated reads that same CLI distribution authority and
  Root through `resolve_runtime_app`, passes its exact Plan to Kernel, and
  replaces only the Driver/socket entry. The test authors no routes or bindings.
- Mixed source App: Rust HTTP → two TS Instances → typed TS dependency, real
  Native and workerd requests, clean exits and identical logical graph after
  explicit execution-class/profile/facility/artifact lowering.
- Console/Auth App: official Shell and JavaScript asset HTTP 200; missing and
  invalid credentials HTTP 401; original Auth-backed protected route and Console
  session HTTP 200 for the same test user; clean shutdown. Original Auth's
  PostgreSQL lifecycle/revocation, signing mismatch and explicit-setup tests pass.
- Core affected checks: discovery, target lowering, generated linkage/provenance,
  locked source dependency identity, scope cancellation/physical drain/fencing,
  and real shared-Cargo-target cfg collision regression. Full Engine-App library
  uses a task-owned Linux subreaper because container PID 1 does not reap
  orphaned test descendants; no lifecycle implementation or test was skipped.

Console production Shell typechecking/bundling passed. A fresh composed browser
render/HMR check is unpassed here: Playwright's browser CDN returned HTTP 403
`Domain forbidden`; access was not bypassed. The inherited dev-loop owner's
separate Vite/Chromium sample remains separately attributed in its evidence.
Neither result qualifies deployed infrastructure.

The candidate CI pins the actual JS source commit
`b6371aebfdf0b1a7056e0121ab6d0e7cb5206b73` and runs the actual shared-source App
checks in addition to existing conformance. Local checks do not replace exact
candidate CI or main read-back evidence, which must be recorded after completion.
