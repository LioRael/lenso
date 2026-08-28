# Legacy framework migration and retirement plan

## Status and source baseline

This document classifies the final v0.3.x framework at legacy commit
[`79ea3e59`](https://github.com/LioRael/lenso/tree/79ea3e59446c56923c2707842e3ffcdd1e7b64c2)
against the executable vNext evidence present at `next` commit
[`4f090814`](https://github.com/LioRael/lenso/tree/4f090814c575ae2e2766bd0155e73857b353bb63).
It is a cutover plan, not a compatibility promise and not evidence that an
unimplemented product has shipped.

The branch statements below describe the pinned pre-cutover baseline. The
cutover has since made `main` the vNext delivery line; `next` is retained as a
pre-cutover integration reference, and `lenso@0.3.47` plus Git history retain
the final v0.3.x source. Historical source paths below still refer to the pinned
legacy commit even though those files deliberately do not exist in the vNext
workspace. Git history, not a copied `legacy/` directory, is the forensics
source.

Current executable evidence follows the repository owners established by ADR
0064: portable core remains here; Rust Runner and host evidence lives in
[`lenso-runtime-rust`](https://github.com/LioRael/lenso-runtime-rust/tree/0e9d2dc446dbcd30912cfe018ebf1f55cd7de893);
protocol generation lives in
[`lenso-protocols`](https://github.com/LioRael/lenso-protocols/tree/f8575ab93a6442dca96e02d4785db6f25f70846b);
Bun transport lives in
[`lenso-bun-adapter`](https://github.com/LioRael/lenso-bun-adapter/tree/563c1df87f3c1a9cd48123d0f6254770e82c4892);
and authoring lives in
[`lenso-cli`](https://github.com/LioRael/lenso-cli/tree/749f42ac45efe1bce3275e6c8ca964653464fb7d).
The complete pre-extraction tracer-bullet workspace remains available at
[`67d21499`](https://github.com/LioRael/lenso/tree/67d21499548d07e92c2f6529d7c8345e58c067d9).

Every row has exactly one primary vNext owner or an explicit retirement
decision:

- **Migrate** preserves valuable behavior behind a canonical vNext public seam.
- **Adapt** permits a bounded bridge outside Kernel for a named cutover user.
- **Retire** preserves history but carries no runtime contract into vNext.
- **Defer** records motivation without authorizing implementation.

No migration or Adapter may add discovery, remote placement, runtime package
installation, mutable bindings, a Kernel database, built-in product behavior,
or a Control Plane.

## Repository-wide ownership inventory

| Legacy public contract or runtime responsibility | Decision | Canonical vNext owner | Package and behavior impact |
| --- | --- | --- | --- |
| `crates/lenso`: `Host`, public facade, linked Modules, Console and System Plane re-exports | Migrate | App authoring facade over `lenso-authoring`, `lenso-app-plan`, and the thin Runner | Replace Host builders with App Composition plus a Resolved App Plan. Do not re-export product Modules or runtime internals from a future facade. |
| `platform-core`: process-wide `AppContext`, configuration, PostgreSQL pool, migrations, Outbox, idempotency, Redis, telemetry configuration | Retire as a shared platform layer | Module-owned configuration and persistence; explicit optional Modules where semantics are deep | Split by data owner. No process-wide database, migration runner, Outbox, cache, or telemetry singleton enters Kernel. |
| `platform-module`: legacy Module lifecycle, feature hooks, manifests, scheduled functions, admin and Story integration | Migrate | Minimal Module factory and `prepare`/`activate`/`deactivate` lifecycle through an Execution Adapter | Retain only lifecycle-independent domain behavior. HTTP, scheduling, migration, Story, and admin hooks become explicit Capabilities or owner-local code. |
| `platform-provider`: Provider registry, endpoint resolution, descriptor fetch, live digest checks, credentials, remote Provider calls | Migrate endpoint execution; retire registry semantics | Execution Adapter plus exact pre-boot Capability binding and Adapter handshake | Native endpoints are statically linked; Bun endpoints use the selected process Adapter. No global lookup, fallback Provider, or live descriptor discovery. |
| `platform-http`: Axum stack, request context, actor extraction, error mapping, CORS, health and OpenAPI serving | Migrate selectively | Protocol/Ingress Module or Browser Adapter; Auth Capability; target business Capability | Protocol parsing and status mapping stay outside Kernel. Replace ambient actor/admin scopes with credential evidence, sealed ActorAssertions, and target-owned authorization. |
| `platform-runtime`: database-backed functions, cron, workers, retries, execution logs and terminal events | Retire as universal runtime | Ordinary Scheduler, Worker, Workflow, durable queue, or audit Modules selected only by an App that needs them | Kernel managed tasks are volatile and bounded. Durable scheduling, retries and logs require explicit product semantics and owner-owned storage. |
| `platform-runtime-observability`: revisioned snapshots and resumable runtime feed | Migrate structural facts; retire correctness claims | Non-blocking Runtime Diagnostics; optional `lenso-otel-module`; explicit durable Audit or Story Module | Diagnostics are bounded, lossy, non-blocking and payload-free. A durable feed must consume explicit business or audit Events instead. |
| `platform-runtime-operations`: durable retry/control operations and operation journal | Retire as generic runtime authority | Domain-specific Capability or a separately approved operational Module | There is no ambient retry, restart, scale, or mutation authority. Dangerous controls require a concrete product and separately bounded Capability. |
| `platform-system-plane`: enrollment, discovery, topology, module/runtime operations and system-wide state | Retire from local v1; defer distributed motivation | No v1 owner; future direction only | Do not port its APIs, database, agents, reconciliation loop, or synchronous dependencies. Established Apps run from immutable local Plans. |
| `platform-module-management` and `lenso-module-management`: catalog, install state, approvals, desired composition, Provider plans, lock snapshots and operation journal | Migrate authoring behavior; retire runtime catalog | `lenso-authoring`, ordinary package managers and lockfiles | Preserve reviewable add/check/resolve diffs and deterministic locks. Remove durable install state, runtime reconciliation and Kernel admission. |
| `lenso-contracts`: legacy Manifest, Module Release, Console Surface, lifecycle, cron, HTTP, Story display and admin contracts | Split: migrate domain contracts; retire privileged sections | Capability Descriptor and package-local JSON Schemas; ordinary Module contracts | Re-author only contracts with a named consumer. Package version and Capability version remain independent; feature-specific Manifest sections do not survive. |
| `lenso-api-contracts`: generators for System Plane, delivery, GA, extraction, Console, Service and catalog artifacts | Retire as one generator surface | `lenso-contract-codegen` for portable Capabilities; owner-specific tooling for retained products | Do not regenerate obsolete aggregate schemas. Retained schemas move with their owning Module or authoring tool and receive conformance tests there. |
| `lenso-service`: Service/Autonomous Service contracts, context model, compatibility engine, direct HTTP/gRPC bindings, workload identity, delivery and extraction plans | Split: migrate contract behavior; retire Service model | Portable Capabilities, sealed Invocation Context extensions, Auth Modules, protocol Adapters, and owner-specific migration tools | Preserve wire and authorization behavior only where a vNext consumer exists. `Service`, `ServicePrincipal`, universal context, deployment and extraction are not vNext runtime nouns. |
| `lenso-autonomous-service`: PostgreSQL runtime, NATS transport, inbox/outbox, durable Workflow, Story feed, dead-letter operations and System Plane endpoints | Split into optional products | Stateful Module or private persistence Adapter; broker/Workflow/Story Modules; protocol Adapter | No aggregate Autonomous Service runtime is ported. Each retained concern owns its data, Capability and failure semantics independently. |
| `lenso-api`, `lenso-worker`, `lenso-migrate`, `lenso-bootstrap` | Replace executables by responsibility | Thin App Runner, protocol/Worker Modules, owner-specific setup/upgrade commands, and authoring tooling | An App selects only the executables and Modules it needs. Boot never runs irreversible migrations and no bootstrap process discovers a second graph. |
| `lenso-operator` and legacy Kubernetes CRDs | Retire from v1; defer deployment products | External deployment tooling; no Kernel or App owner | Kubernetes reconciliation, workload replicas, rollout and remote placement are not vNext runtime mechanisms. This does not retire ADR 0063's Plan-declared local Execution Lane placement. |
| `fixtures/provider`: Provider v1 descriptor/health, locked digest, durable invocation and HTTP/gRPC parity proof | Split: migrate logical contract checks; retire the generic Provider protocol | Execution Adapter handshake and portable Capability conformance; domain-owned idempotency or Workflow Module | Exact endpoint tables fail preparation before traffic. Generic Provider health, live digest fetch and Adapter-owned durable replay are retired; request/Stream/Event behavior is retested at the Capability seam. |
| `platform-testing` and black-box fixtures | Migrate behavior, not helpers or names | Public vNext App/Kernel seam plus portable contract conformance suites | Re-express useful assertions against Plans, Capabilities, Drivers and Adapters. Do not preserve Host, Service or Provider test APIs for convenience. |
| TypeScript Service Kit parity and Service-oriented SDKs | Retire as a peer authoring model | Generated TypeScript Capability bindings and a Bun Module SDK around the same lifecycle | Cross-language parity is measured at Capability and lifecycle behavior, not by reproducing the Rust Service framework. |
| Repository `skills/`: Service, Autonomous Service, Console Surface, release, extraction, recovery and legacy client workflows | Split by the skill inventory below | vNext authoring skill or explicit retirement | Skills are user-facing contracts. A skill cannot preserve a retired runtime noun or imply that deferred or documentation-only behavior is implemented. |

Aggregate legacy crates mixed several responsibilities. The following split is
normative when a crate-level row names more than one outcome; each responsibility
has one owner.

| Legacy runtime responsibility | One canonical owner or decision |
| --- | --- |
| App graph, exact bindings, lifecycle state, readiness, bounded invocation, cancellation and supervision | Kernel mechanism |
| Local scheduling, monotonic time, timers, task wakeups, restart jitter and host shutdown translation | Runtime Driver |
| Module generation, endpoint installation, cross-runtime wire validation, process lifecycle and physical isolation semantics | Execution Adapter |
| Package selection, lockfile inspection, schema/codegen checks, Composition editing and Plan materialization | Authoring tool |
| HTTP/gRPC/WebSocket/game framing, credential selection and protocol response mapping | Protocol/Ingress Module |
| Authentication and assertion issuance | Auth Module |
| Final business authorization | Target business Module |
| Stateful schema, transaction scope, backup, restore and recovery | The stateful Module that owns the data |
| Database setup and irreversible upgrade | Owner-specific authoring/operations command |
| Durable queue, Outbox, inbox, dead-letter and broker delivery | Optional broker or Outbox Module |
| Durable scheduling, retries, Workflow, idempotency and compensation | Optional Scheduler, Worker or Workflow Module |
| Business Story and audit history | Story or Audit Module consuming explicit business/audit Events |
| Technical trace, metric and log export | Optional observability Module such as `lenso-otel-module` |
| Route/navigation/asset assembly | Web Shell Module |
| Browser-to-App transport and generated-client projection | Browser Adapter Module |
| App-specific browser contribution metadata and assets | UI Contribution Module |
| Package/release provenance policy before boot | Optional external supply-chain or authoring policy tool |
| Runtime package catalog, discovery, graph mutation, fallback binding and hot installation | Retire from v1 |
| Plan-declared local Execution Lane placement and replicated single-owner Kernel lanes | Resolved App Plan data plus the native Runner; ADR 0063's Request/Stream/Event conformance, terminal propagation, and scaling gates are implemented in `lenso-runtime-rust` |
| System Plane enrollment, discovery, remote placement, workload replicas, reconciliation and remote execution | Defer; no current owner or implementation |
| Kubernetes deployment and release orchestration | External deployment tooling, not Lenso Kernel |

## Legacy contract-family decisions

The pinned legacy `contracts/` tree is classified by public family so no
aggregate artifact silently survives through a crate-level decision.

| Legacy contract family | Decision | Replacement or retirement rule |
| --- | --- | --- |
| `modules/lenso.module-manifest` | Migrate selected identity, entrypoint, configuration and Capability declarations | App project inputs plus Module and Capability Descriptors. Feature-specific release, HTTP, Surface, Story and migration fields are retired. |
| `modules/lenso.module-release`, catalog verification profiles/receipts, linked provenance and digest admission | Retire from Kernel and initial authoring path | Cargo/npm/Bun/OCI locks own exact artifacts. Optional supply-chain policy may validate project inputs before Plan materialization, but cannot become Kernel admission. |
| management install, approval, service-installation, desired-composition and operation-journal schemas | Migrate only reviewable project changes | `lenso-authoring` add/check/resolve and ordinary version control. Runtime install state and reconciliation are retired. |
| compatibility matrices for HTTP, gRPC, Events, config, reliability and Workflow | Split | Portable Capability evolution is linted by `lenso-contract-codegen`. Product-specific compatibility belongs to the owning Module. There is no universal runtime compatibility engine. |
| Service v2, Autonomous Service and System graph schemas | Retire | App Composition and the Resolved App Plan are the only vNext graph authority. Remote topology remains deferred. |
| System Plane enrollment, observability, runtime/module operations and core schemas | Retire | No v1 replacement. Diagnostics are local and read-only; authoring changes happen before boot. |
| Console Module, Console UI ESM, Surface grants and Console contract vectors used for one target's own pages | Migrate to target-owned App Web UI | `lenso.ui.contribution@1`, `lenso.web.shell@1`, generated Browser clients and target-owned App bindings. Digest-bound ambient Console authority is retired. |
| Console contracts used for remote/multi-target operation, independent operator identity, durable cross-target state or an independent release lifecycle | Defer to the cross-App shape | A separately composed Console App and #601 allowlisted Connector only after ADR 0060 entry criteria are accepted. Target-advertised UI is never executed automatically. |
| `openapi/`, `grpc/`, `protobuf/`, `errors/` and HTTP service bindings | Adapt only for named external clients | A protocol Module may preserve an existing edge contract and error envelope while invoking explicitly bound Capabilities. The transport schema never becomes the Capability source of truth. |
| common context, Story context, tenant, trace, idempotency and delegated actor documents | Split by meaning | Kernel Caller Module/request/deadline/cancellation plus opaque or sealed extensions. Auth, telemetry, tenant, Story and idempotency remain domain-owned. |
| event envelopes, inbox/outbox and broker contracts | Migrate domain Events only | Kernel Events remain volatile. Durable delivery, replay and broker envelopes require an explicit broker or Outbox Module. |
| Workflow definitions, compatibility and compensation artifacts | Migrate only when an App selects a Workflow Module | The Workflow Module owns persistence, timers, versioning, idempotency and compensation; Kernel provides no durable workflow semantics. |
| delivery plans, promotion, rollback, canary, policy and workload-control artifacts | Retire from the runtime | Keep deployment and release operations in external tooling until a separate product is accepted. A Resolved App Plan is not a Service Release. |
| GA support, disaster recovery, extraction and contract-retirement artifacts | Retire as universal framework contracts | Recreate only owner-specific runbooks or migration commands with a named product and data owner. Documentation alone is not executable recovery evidence. |
| Kubernetes CRDs and operator fixtures | Retire from v1 | Plan-declared local Execution Lane placement does not imply Kubernetes workload placement, workload replicas or a reconciliation API. |

## Legacy agent-skill decisions

The pinned `skills/` tree is part of the public authoring experience and must
not route users back into retired concepts.

| Legacy skill family | Decision |
| --- | --- |
| `lenso-start`, `lenso-business-planning`, `lenso-app-composition` | Rewrite around App, Module, Capability, package-manager inputs and immutable Plan generation. The vNext versions delivered through `main` are the replacement path. |
| `lenso-module-authoring` | Rewrite around minimal Module lifecycle and generated Capability contracts. The vNext skill must not emit Manifest hooks, Surfaces, migrations or a private registry. |
| `lenso-api-client` and `lenso-contract-evolution` | Rewrite for generated Capability clients, Descriptor SemVer, portable values and owner-specific protocol edges; retire Service discovery and universal compatibility matrices. |
| `lenso-starter-host` | Retire the Host workflow; any replacement scaffolds a thin App Runner and reviewable Composition/Plan. |
| `lenso-service-authoring` and `lenso-autonomous-service-authoring` | Retire. Future distributed product authoring requires a new accepted architecture and cannot alias these skills. |
| `lenso-console-surface-authoring` | Split: target-owned pages become UI Contribution authoring; cross-App Console guidance remains unavailable until ADR 0060/#601 entry criteria are met. Retire Surface grants and target-pushed Console ESM. |
| `lenso-durable-workflow` | Retire as a framework-default workflow. Reintroduce only for a selected Workflow Module with owner-specific persistence and recovery semantics. |
| `lenso-module-extraction` | Retain only as bounded migration guidance for a named application; it cannot install or extract Modules at runtime and is removed when that cutover closes. |
| `lenso-incident-recovery` | Replace universal platform recovery with App-, Module- and data-owner runbooks. Runtime Diagnostics alone cannot claim recovery evidence. |
| `lenso-reviewed-release` | Keep only in the v0.3.x `main` release lane until that support window closes. A future vNext release skill needs an independently accepted publication contract. |

## Valuable black-box behavior to retain

Legacy tests are evidence of user-visible intent, not APIs to copy. The
following behaviors remain valuable when restated at vNext public seams.

| Valuable behavior | vNext restatement and evidence |
| --- | --- |
| Complete ordered startup, readiness, failure rollback and bounded shutdown | Feed one immutable Plan to Kernel, observe lifecycle and terminal outcomes through its public App seam. See [`lenso-kernel` lifecycle tests](../../crates/lenso-kernel/tests/lifecycle.rs) and [`shutdown.rs`](../../crates/lenso-kernel/tests/shutdown.rs). |
| Explicit dependencies and replaceable implementations | Change only App Composition/Plan bindings and keep the consumer unchanged. See the pinned [`vnext-native-greeter`](https://github.com/LioRael/lenso/tree/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-native-greeter) proof and current [`lenso-authoring` acceptance](https://github.com/LioRael/lenso-cli/blob/749f42ac45efe1bce3275e6c8ca964653464fb7d/crates/lenso-authoring/tests/authoring.rs). |
| Provider outage, bounded restart and no replay | Stable handles report `Unavailable`, generations advance only after recreation, and in-flight work is not retried. See [`supervision.rs`](../../crates/lenso-kernel/tests/supervision.rs). |
| Machine-readable business refusal versus runtime failure | Capability Domain Errors remain separate from Runtime Failures across native and Bun paths. See [`provider_failures.rs`](https://github.com/LioRael/lenso-protocols/blob/f8575ab93a6442dca96e02d4785db6f25f70846b/crates/lenso-contract-codegen/tests/provider_failures.rs) and current [Bun conformance](https://github.com/LioRael/lenso-bun-adapter/tree/563c1df87f3c1a9cd48123d0f6254770e82c4892/crates/lenso-bun-adapter/tests). |
| Contract generation, artifact freshness and additive evolution | Generate Rust and TypeScript from one Capability Descriptor and reject drift or incompatible evolution before boot. See [`codegen.rs`](https://github.com/LioRael/lenso-protocols/blob/f8575ab93a6442dca96e02d4785db6f25f70846b/crates/lenso-contract-codegen/tests/codegen.rs). |
| Exact Provider descriptor/digest checks and HTTP/gRPC operation parity | Preserve fail-closed exact Capability identity, Descriptor version and Operation-table checks in the Adapter handshake, then run transport-independent Request/Stream/Event conformance. Retire live Provider discovery, generic health and Adapter-owned durable invocation replay. See [Bun Adapter tests](https://github.com/LioRael/lenso-bun-adapter/tree/563c1df87f3c1a9cd48123d0f6254770e82c4892/crates/lenso-bun-adapter/tests) and [contract-codegen tests](https://github.com/LioRael/lenso-protocols/tree/f8575ab93a6442dca96e02d4785db6f25f70846b/crates/lenso-contract-codegen/tests). |
| Bounded queues, cancellation and transport-neutral interaction semantics | Assert Request, Stream and Event outcomes at the Capability seam, including partial Event admission and one Stream terminal outcome. See core invocation tests and [`bun_cross_runtime.rs`](https://github.com/LioRael/lenso-bun-adapter/blob/563c1df87f3c1a9cd48123d0f6254770e82c4892/crates/lenso-bun-adapter/tests/bun_cross_runtime.rs). |
| Protocol credentials become authenticated domain authority and the target decides authorization | Ingress selects credential evidence, Auth issues a sealed assertion, the target projects its Actor and performs final authorization. See the pinned [`vnext-game-session`](https://github.com/LioRael/lenso/blob/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-game-session/tests/game_session.rs) proof. |
| Durable state survives restart and migration is explicit | A stateful Module owns schema/setup/upgrade/recovery and fails closed when storage is unavailable. See the pinned [`vnext-stateful-module`](https://github.com/LioRael/lenso/blob/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-stateful-module/tests/durable_state.rs) proof. |
| Durable business narrative is independent from telemetry | Story consumes explicit business Events, is removable, and survives restart without Runtime Diagnostics. See the pinned [`vnext-story-module`](https://github.com/LioRael/lenso/blob/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-story-module/tests/durable_story.rs) proof. |
| Observation cannot block or alter the App | Runtime Diagnostics are bounded and lossy; the optional OTel Module owns export and trace propagation. See core [`diagnostics.rs`](../../crates/lenso-runtime-conformance/tests/diagnostics.rs) and current [`lenso-otel-module` tests](https://github.com/LioRael/lenso-otel-plugin/tree/856190e128605479becb484a790368307085428c/crates/lenso-otel-module/tests). |
| App-specific Web UI is removable and least-authority by composition | Web Shell, Browser Adapter and UI Contribution are selected pre-boot; generated clients expose only declared resolved requirements. See the pinned [`vnext-web-ui`](https://github.com/LioRael/lenso/blob/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-web-ui/tests/web_ui.rs) proof. |
| Agent tools, memory and models remain replaceable product capabilities | The Agent Harness is an ordinary optional composition, not a Kernel mode. See the pinned [`vnext-agent-harness`](https://github.com/LioRael/lenso/blob/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-agent-harness/tests/harness.rs) proof. |

The following legacy tests are not blanket retention requirements: database
table layouts, process-wide migration order, generic admin scopes, global
Provider discovery, Service Release admission, System Plane enrollment,
Kubernetes reconciliation, universal Worker/Outbox behavior and Console
Surface grants. A product that still needs one of those outcomes must first
name its Module, Capability or external tool and then port the black-box intent.

## Executable evidence and maturity

| Migration decision | Current evidence | Maturity boundary |
| --- | --- | --- |
| Native typed Modules and provider replacement | Current [`lenso-native-adapter`](https://github.com/LioRael/lenso-runtime-rust/tree/0e9d2dc446dbcd30912cfe018ebf1f55cd7de893/crates/lenso-native-adapter) and pinned [`vnext-native-greeter`](https://github.com/LioRael/lenso/tree/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-native-greeter) | Executable tracer bullet; native Rust remains statically linked in v1. |
| Bun process Modules and portable Request/Stream/Event contracts | Current [`lenso-bun-adapter`](https://github.com/LioRael/lenso-bun-adapter/tree/563c1df87f3c1a9cd48123d0f6254770e82c4892/crates/lenso-bun-adapter) and portable conformance fixtures | Executable tracer bullet; trusted child process, not a sandbox or remote runtime. |
| Browser and WASIp2 Runtime Drivers | Current [`lenso-browser-driver`](https://github.com/LioRael/lenso-runtime-rust/tree/0e9d2dc446dbcd30912cfe018ebf1f55cd7de893/crates/lenso-browser-driver), [`lenso-wasip2-driver`](https://github.com/LioRael/lenso-runtime-rust/tree/0e9d2dc446dbcd30912cfe018ebf1f55cd7de893/crates/lenso-wasip2-driver), issue [#585](https://github.com/LioRael/lenso/issues/585) | Real Chrome and Wasmtime lifecycle smoke, not one universal Wasm artifact ABI. WASIp2 remains experimental and unpublished. |
| Native replicated Execution Lanes and local placement | [ADR 0063](../adr/0063-scale-native-apps-across-replicated-kernel-lanes.md), current [lane tests and evidence](https://github.com/LioRael/lenso-runtime-rust/tree/0e9d2dc446dbcd30912cfe018ebf1f55cd7de893/crates/lenso-runner/tests) | Implemented Request/Stream/Event transfer, terminal propagation, and checked scaling evidence; no work stealing or live migration. |
| Optional OTel, Story, protocol/game, Agent and target Web UI products | Current [`lenso-otel-module`](https://github.com/LioRael/lenso-otel-plugin/tree/856190e128605479becb484a790368307085428c) plus pinned pre-extraction [`vnext-story-module`](https://github.com/LioRael/lenso/tree/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-story-module), [`vnext-game-session`](https://github.com/LioRael/lenso/tree/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-game-session), [`vnext-agent-harness`](https://github.com/LioRael/lenso/tree/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-agent-harness), and [`vnext-web-ui`](https://github.com/LioRael/lenso/tree/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-web-ui) proofs | Executable examples proving ownership and removability, not production-complete products. |
| Cross-App Console Connector | Deferred issue [#601](https://github.com/LioRael/lenso/issues/601) and [ADR 0060](../adr/0060-compose-target-web-ui-in-app-and-separate-cross-app-console.md) | No implementation is authorized until a real remote/multi-target, independent identity/state, or release-lifecycle requirement is accepted. |
| Distribution and microservices | [`distributed-module-runtime.md`](future-directions/distributed-module-runtime.md) | Motivation only. Discovery, remote placement, replicas, reconciliation and Control Plane remain out of scope. |

## Bounded compatibility layers

Compatibility exists to move named users; it is never a default dependency of
Kernel or every App.

| Layer | Explicit users | Boundary | Removal criterion |
| --- | --- | --- | --- |
| Final v0.3.x release | Consumers of the published `lenso`, `lenso-contracts`, `lenso-module-management`, `lenso-bootstrap`, `lenso-api`, `lenso-migrate`, `lenso-worker`, `lenso-service`, `lenso-autonomous-service`, `lenso-platform-core`, `lenso-platform-module`, `lenso-platform-module-management`, `lenso-platform-provider`, `lenso-platform-http`, `lenso-platform-runtime`, `lenso-platform-runtime-observability`, `lenso-platform-runtime-operations`, `lenso-platform-system-plane`, `lenso-platform-testing` crates and `@lenso/service-kit` package | Tagged source, binaries, dependency graph and releases; no linking into the vNext workspace. Registry versions, tags and changelogs are the package-coordinate evidence. | Remove support only after every listed coordinate is deprecated under its published support window, known first-party consumers have migrated or accepted retirement, and any remaining external consumer can pin an independently maintained release or fork. |
| Legacy edge-contract Adapter | A named application whose external HTTP/gRPC clients cannot switch atomically | App-selected protocol Module maps one pinned legacy edge contract to explicit generated Capability clients; no registry, ambient admin authority or business ownership | Delete after traffic and client telemetry show no use for the agreed compatibility window and rollback has expired. No current App authorizes this layer by default. |
| Owner-specific PostgreSQL export/import command | The named stateful Module receiving records from one legacy schema | Offline or quiesced authoring/operations tool; verifies source version, produces a reviewable report and invokes the new owner's explicit setup/import seam | Delete after backup retention and rollback windows close, migrated counts/checksums reconcile, and the owner has exercised restore from the new format. |
| Legacy durable-event bridge | A named producer/consumer pair that must overlap during cutover | Optional broker/Outbox Module translates one versioned domain Event; Kernel Event semantics remain volatile and unchanged | Delete after all producers and consumers use the new Capability/Event contract and the old queue is drained under the owner's policy. |

New compatibility work requires an issue naming the user, old and new
contracts, authority boundary, observability, rollback window and deletion
date. “Potential ecosystem compatibility” is not a user.

## Data migration ownership

| Legacy data | New owner | Migration rule |
| --- | --- | --- |
| Shared platform tables and process-wide migration ledger | None by default | Inventory every table by business meaning. Assign it to a selected Module or retire it; never import the platform schema wholesale. |
| Module business state | The corresponding stateful Module | Its explicit setup/import/upgrade command owns validation, transactions and recovery. Preparation verifies but does not apply irreversible production migration. |
| Auth sessions, credentials and revocation data | Selected Auth Module | Migrate through an Auth-owned command. Raw credentials never enter Plans, diagnostics or a generic migration service. |
| Outbox, inbox, dead-letter and broker offsets | Selected broker/Outbox or domain Workflow Module | Migrate only when durable delivery is still a product requirement. Preserve domain idempotency keys, not Kernel request IDs. |
| Function runs, cron state and Workflow instances | Selected Scheduler/Worker/Workflow Module | Pin the old definition and state version, quiesce claims, import through owner tooling, then resume. Generic runtime tables are not a Kernel concern. |
| Story segments and durable audit evidence | Story or Audit Module | Import explicit business/audit records with owner-defined identity and ordering. Do not reconstruct correctness history from lossy Runtime Diagnostics. |
| System Plane topology, enrollment, desired state and operation journals | Retire unless a separately accepted deployment product names a durable need | Do not import into App Composition or Kernel. Keep an archival export when required for operations or compliance. |
| Runtime observability snapshots and technical logs | Telemetry backend or archival tooling | No operational-state import into Kernel. Retain externally according to the application's observability policy. |

Each data owner must publish source schema/version, quiescence rule, backup,
count or checksum reconciliation, idempotent retry behavior, restore test and
point of no return before a production migration is approved.

## Package and example-App transitions

1. Replace each legacy aggregate package with the smallest set of App-owned
   package-manager dependencies and explicit Composition entries. Do not create
   package-name shims that conceal old lifecycle or registry semantics.
2. Generate Capability bindings from one Descriptor source and install them in
   Rust and Bun consumers through their ordinary package managers.
3. Transition a simple linked-module or first-user App to
   pinned [`vnext-native-greeter`](https://github.com/LioRael/lenso/tree/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-native-greeter)
   shape first.
   Its rollback is the unchanged v0.3.x binary and data because no shared
   runtime state is mutated.
4. Replace the legacy `fixtures/provider` proof with the native/Bun Adapter
   handshake and portable Capability conformance suites. Do not preserve its
   descriptor endpoint, generic health API, HTTP/gRPC wire, or durable replay
   store as a compatibility protocol.
5. Transition stateful behavior through
   pinned [`vnext-stateful-module`](https://github.com/LioRael/lenso/tree/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-stateful-module),
   then add only
   the optional Story, OTel, protocol, Agent or Web UI packages the App needs.
6. A target-owned page follows the pinned
   [`vnext-web-ui`](https://github.com/LioRael/lenso/tree/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-web-ui),
   not legacy Console Surface packaging. A cross-App Console waits for #601's
   entry criteria.
7. Release automation for v0.3.x stays on `main`. A future vNext release lane
   must be accepted independently; a Plan must never be renamed into a Module
   or Service Release to reuse old publication machinery.

## Staged cutover and rollback points

### Stage 0: freeze an auditable baseline

- Pin the legacy application commit, lockfiles, schemas, database version,
  external edge contracts and representative black-box results.
- Name every application, package and data owner. Unowned behavior is a
  retirement candidate, not an implicit Kernel requirement.
- **Rollback:** none is needed; this stage is read-only.

### Stage 1: compose the vNext application without production traffic

- Add packages, Module entrypoints, configuration references and every
  Capability binding through authoring tooling.
- Materialize and review the immutable Plan. Validate exact Descriptor and
  execution-class availability before boot.
- **Rollback:** discard the vNext project/Plan diff. The legacy application and
  data remain untouched.

### Stage 2: prove behavior at public seams

- Run native/Bun/Driver conformance and the App-specific retained black-box
  cases. Add optional product Modules only when their absence changes a stated
  requirement.
- Compare domain outcomes, authority decisions and externally visible protocol
  behavior, not internal crate or table shape.
- **Rollback:** stop the vNext shadow instance and continue the pinned legacy
  binary. No production writer has moved.

### Stage 3: migrate owner-controlled data

- Quiesce the legacy owner, take and verify a restorable backup, run the new
  owner's explicit import/upgrade command, reconcile counts/checksums and run a
  restore rehearsal.
- Avoid bidirectional dual-write. If overlap is unavoidable, it requires a
  separately reviewed idempotent bridge with one authoritative writer.
- **Rollback:** before the declared point of no return, stop vNext, restore or
  reselect the untouched legacy store, and restart the legacy owner. After that
  point, rollback means a tested reverse export owned by the same Module, not a
  Kernel mechanism.

### Stage 4: move clients and traffic

- Shift one bounded cohort through the selected protocol Adapter. Preserve the
  target Module's final authorization and monitor domain/runtime outcomes
  separately.
- Keep the legacy endpoint available only for the declared compatibility
  window; do not let it discover new bindings or mutate the Plan.
- **Rollback:** route the cohort back to the legacy binary while the data
  authority selected in Stage 3 remains consistent with the rollback runbook.

### Stage 5: make vNext authoritative

- Stop legacy writers and workers, drain owner-defined durable queues, archive
  required evidence, and start vNext only from the reviewed Plan and locks.
- Exercise App restart, provider failure, backup/restore and bounded shutdown
  before ending the rollback window.
- **Rollback:** use the application-specific backup/reverse-export path. Do not
  re-enable two authorities or ask Kernel to rebind dynamically.

### Stage 6: remove the legacy path

- Delete compatibility Adapters, old package dependencies, deployment entries,
  secrets, tables and runbooks only after their individual removal criteria are
  met.
- Retain immutable ADRs, tags and Git history for forensics.
- **Rollback:** restore from version control only while the old contract and
  data format remain within their published support window; otherwise treat
  reintroduction as a new migration decision.

## Deletion candidates and gates

| Candidate | Required condition before removal |
| --- | --- |
| v0.3.x `lenso` facade and all `platform-*` packages | No supported App imports their public APIs; retained behaviors pass at vNext public seams; support window is closed. |
| `lenso-service`, `lenso-autonomous-service`, `lenso-api`, `lenso-worker`, `lenso-migrate`, `lenso-bootstrap` | Every named executable user has migrated or accepted retirement; owner-specific data and edge-contract cutovers are complete. |
| `lenso-module-management`, `platform-module-management`, release/catalog/approval schemas | App projects use ordinary package locks plus vNext authoring; no runtime install-state consumer remains. |
| `platform-system-plane`, runtime/module operations, operator and CRDs | No supported deployment depends on their endpoints or reconciliation; archival operational evidence is retained where required. Future distribution does not block deletion. |
| Console Surface manifests, grants, ESM artifacts and admin namespaces | Target pages use UI Contributions and generated clients; any future independent Console uses separately selected Console packages and allowlisted Connectors. |
| Shared platform migrations and PostgreSQL schemas | Every table has an owner, verified migration or explicit retirement; backups and compliance retention are satisfied. |
| Aggregate legacy contract generator and generated artifacts | Every retained external contract has moved to its owner and all obsolete consumers are gone. |
| Legacy release workflows and publication configuration | v0.3.x publication is ended and an independently reviewed vNext release process exists. |

Deletion is not gated on implementing remote placement, discovery, replicas,
reconciliation, a Control Plane, or the deferred cross-App Console Connector.
Those are future product decisions, not reasons to keep the legacy runtime
architecture alive.
