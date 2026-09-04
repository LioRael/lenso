# Plugin authoring design: approved baseline

Status: **Design approved on 2026-09-04 by the repository owner; implementation pending.**
Date: 2026-09-04.

This is the approved design entrypoint following the
[approved overall direction](2026-09-04-plugin-authoring-and-lifecycle.md).
Accepted ADRs remain normative, including ADRs 0073 and 0074 with explicit
adoption rules. The owner approved this design set, including lifecycle,
multilingual authoring, product SDK, and first-delivery boundaries. Macro syntax,
file representation, and exact profile versions still require implementation
specifications. Approval is not a shipped SDK/runtime claim or a release action.
[Issue #695](https://github.com/LioRael/lenso/issues/695) tracks delivery.

[Implementation specification #699](https://github.com/LioRael/lenso/issues/699)
records the concrete interfaces and version boundaries proposed against the
verified implementation baseline. Its review and delivery status is authoritative
for those details; the examples below remain the approved design illustrations.

| Review topic | Focused companion |
| --- | --- |
| Natural Rust/TS authoring, from minimal to resource-owning Plugins | [Language examples](2026-09-04-multilingual-plugin-authoring.md) |
| Product SDK helpers generating standard Capability declarations and bindings | [Declaration pipeline](2026-09-04-plugin-declaration-pipeline.md) |
| Constructor failure, late completion, and safe bounded cleanup | [Cancellation and cleanup](2026-09-04-plugin-cancellation-and-cleanup.md) |
| Stable requirement IDs and saved instance choices | [Accepted ADR 0073](../adr/0073-name-and-persist-plugin-dependencies.md) |
| Host-owned terminal failure impact after readiness | [Accepted ADR 0074](../adr/0074-scope-terminal-failure-to-host-essential-instances.md), [examples](2026-09-04-plugin-fault-scope.md) |
| Existing-source adoption and the first delivery boundary | [Adoption and delivery](2026-09-04-plugin-adoption-and-delivery.md) |

## 1. Keep one Plugin model

A Plugin groups behavior that should be installed, configured, stopped, and
replaced together. It can own tools, routes, listeners, tasks, caches,
connections, and persistent data. State does not create another Plugin category.

Official implementations remain Rust-first. Shared contracts and lifecycle
semantics stay language-independent; TypeScript has its own natural authoring
form rather than emulating Rust ownership. Generated clients, schemas, and error
semantics remain the existing foundation. Language support does not imply every
runtime target supports the same source or SDK facilities.

Native embedding and independent Process installation are delivery choices;
an installable Rust Process starter
is a candidate default, not a requirement for every Host. Wasm and other
implementations expose only the contracts they actually support. Keep ADR 0071
implementation equivalence; common source alone does not prove portability.

Ordinary authors work with objects, configuration, dependencies, and business
operations. Plans, Generations, Slots, and execution mechanics remain internal
or advanced concepts. Their correctness guarantees are retained.

## 2. Construction, lifecycle, and inputs

A Plugin object is ordinary language-owned state. Rust uses fields; TypeScript
may use an object or closure. The generated constructor can supply
declared configuration, resolved clients, and fields with valid defaults. A
custom asynchronous constructor returns a fully initialized object when those
defaults are insufficient. It may create ordinary library resources itself.
Construction that calls dependencies runs only after those providers are active.

Rust constructor parameters select declared input fields by exact private name
and checked type; dependency IDs remain declared once on fields. No positional
or type-only fallback reconnects identically typed clients. TS infers a default
instance from declared config/dependencies when `create` is absent. A custom
factory, synchronous or asynchronous, returns its complete instance instead.
Capability declarations stay outside that factory and handlers receive the constructed
instance. Fresh private state alone can justify a factory; it is not restricted
to resources. No factory or stop hook is required merely to provide operations.

Construction and enablement are distinct semantics. A lifecycle hook operates
on an existing object; custom construction creates it. Hooks are available with
no-op defaults. Authors do not write both merely to satisfy the framework.
Preparation/activation/readiness ordering still governs external admission.
Existing advanced preparation may reserve reversible resources under ADR 0046.

Before construction returns, its factory owns partial resource cleanup. After
successful return, runtime owns lifecycle cleanup, including a later activation
failure. If construction returns after startup cancellation, ownership still
transfers to cleanup; the late object is never activated. Declaration extraction
does not call constructors or initialize
resources. The multilingual companion states these rules for both authoring forms.

Prefer explicit typed constructor inputs over a mandatory generated
`MyPluginInputs` type. Mapping from declarations to inputs must be deterministic,
including two clients of the same type. Wrapper code shares one instance across
its entrypoints; it does not require an author-provided `Clone` implementation
or a private-state copy per operation. Supported recreation constructs a fresh
object for the same logical Instance. A constructed object receives at most one
stop-hook attempt for its lifetime, subject to safe resource access and the
shared cleanup deadline. It is not guaranteed to run after non-cooperative work
or process termination. SDKs observe late completion without reviving admission.

A small optional `PluginContext` can expose instance identity, scoped logging,
and owned task facilities. Configuration and business dependencies stay explicit.
A separate per-invocation `CallContext` carries deadline, cancellation, and
validated invocation information. Neither context is an arbitrary service
locator, permission escalation interface, or App configuration editor.

Generic `definePlugin` accepts Capability `providers`, not Agent-owned `tools`.
The Agent SDK's proposed `tools([...])` helper supplies an ordinary ToolProvider
declaration there; Rust Tool annotations likewise belong to the Agent SDK.
Tool names are product data within ToolProvider, not extra Kernel operations.
Build-time extraction must not import the application module, and offline bundle
admission executes neither Plugin code nor SDK build entrypoints.

## 3. One integrated author example

This block is **illustrative proposed syntax**, not a compile-ready SDK sample.
Imports and product-owned error/client types are omitted. `Mutex` is an ordinary
Rust asynchronous mutex. Macro spelling and exact signatures remain reviewable.

```rust
#[derive(PluginConfig)]
struct Config {
    #[config(min_length = 1)]
    document: String,

    #[config(default = 60, min = 1)]
    interval_seconds: u64,
}

#[plugin]
struct DocumentSync {
    #[config]
    config: Config,

    #[dependency(id = "source")]
    source: StoreClient,

    #[dependency(id = "destination")]
    destination: StoreClient,

    running: Mutex<()>,
}

enum SyncOutcome {
    Updated,
    AlreadyRunning,
}

#[lenso_agent_tool_sdk::tool_provider]
impl DocumentSync {
    #[tool(name = "sync_document")]
    #[schedule(every_seconds = "config.interval_seconds")]
    async fn sync(&self, call: &CallContext) -> Result<SyncOutcome, SyncError> {
        let Some(_guard) = self.running.try_lock() else {
            return Ok(SyncOutcome::AlreadyRunning);
        };

        let document = self.source.read(call, &self.config.document).await?;
        self.destination
            .put(call, &self.config.document, document)
            .await?;

        Ok(SyncOutcome::Updated)
    }
}
```

The product tool SDK owns the tool contract and schema. Scheduling requires a
real supported Host implementation; a one-shot queue does not establish it.
Both triggers call the same method on the same object. No custom constructor
or cleanup hook is needed for this example. Configuration defaults, clients,
and the default mutex suffice; SDK registrations own their cleanup.

Store owns storage semantics. This example assumes replacement at a destination
key with no competing external writer. Its mutex protects this instance only.
Concurrent external writes need a version/conflict policy. `CallContext` is
injected by the runtime, not exposed as a user-supplied tool argument.

## 4. Approved default behavior

| Concern | Target behavior |
| --- | --- |
| Concurrency | One object per instance, with bounded asynchronous concurrency under Host capacity policy. No implicit object copy, whole-Plugin lock, or movement onto arbitrary threads. Business operations declare or implement necessary mutual exclusion. |
| Repeated trigger | The example returns `AlreadyRunning` to a competing manual or scheduled invocation. It does not queue another copy. |
| Periodic work | First run one interval after readiness; subsequent scheduled runs wait one interval after completion. Manual calls do not reset the timer. No offline catch-up or exactly-once promise. |
| Admission and cleanup | Open entrypoints only when ready. Disablement closes new admission, stops future triggers, and bounds in-flight completion/cancellation and cleanup under one App-wide deadline. |
| Configuration | Derive schema/defaults from the typed declaration, with explicit business validation where needed. Each instance observes a coherent configuration value. Validate a replacement before applying it through a supported update path. |
| Calls and errors | Preserve domain errors, unavailability, deadline, and cancellation as structured outcomes. Propagate invocation context without extending its deadline. No hidden write retry or replay. |
| Uncertain effects | Losing a write response or cancelling a call does not prove the write was undone. Report uncertainty; recovery/idempotency belongs to the operation contract. |
| Updates | Choose a supported replacement strategy from actual compatibility and resource constraints. Stop an exclusive old writer before opening its replacement. Neither always-hot replacement nor unconditional whole-App restart is the universal rule. |
| Failed replacement | Validation failure preserves the current instance. Failure after stopping it may leave it unavailable. Restoring code requires compatible data; code rollback is not data rollback. |

Managed cleanup covers SDK-owned registrations and work. Arbitrary threads,
non-cooperative native code, and external side effects do not acquire automatic
cancellation or reversal guarantees. A required local cleanup operation can use
an optional stop hook; durable writes cannot rely solely on that hook running.

Ordinary Event notifications report facts already produced. Keep bounded,
volatile admission and explicit delivery outcomes unless a particular contract
provides stronger guarantees. A decision hook must instead name a Host-owned
extension point with ordering, deadline, and rejection semantics. No universal
interceptor over arbitrary Capability calls; a "continue" decision does not
override required authorization.

## 5. Host, SDK, and storage ownership

| Owner | Responsibility |
| --- | --- |
| Host/runtime | Instance identity, admitted configuration and bindings, actual execution facilities and limits, readiness, lifecycle supervision, and supported update coordination |
| SDK | Typed declarations, generated contracts/clients, schema validation plumbing, registration ownership, logging association, and invocation context propagation |
| Plugin | Business rules, ordinary library resources, private data meaning, transactions, migrations, idempotency, and recovery |
| Shared service provider | Its explicit cross-Plugin contract, including service-specific authorization, durability, or secret rotation where applicable |

Keep all existing storage approaches. A Plugin may use an ordinary HTTP or
database library, its private persistence implementation, or a declared storage
Capability. It does not have to create another Plugin merely to use a database.
No universal `DataDir`, `Storage`, or `State<T>` is required. A future optional
local-directory helper must not become the persistence model. Read-only staged
resource files are distinct from writable business data.

Configuration may contain a secret reference; a declared provider resolves it
under applicable authority. A Host can assemble a default provider without
putting secret storage into Kernel. Secret lifetime/rotation is a service
contract, not automatic mutation of a resolved configuration value.

A restricted-looking SDK does not sandbox native library access. Process
separation alone does not enforce filesystem/network permissions. Wasm can
access only the imports the selected Host actually supplies. Supported APIs
and enforced restrictions must be checked for the exact execution profile.

## 6. Dependency injection and multiple instances

Dependency injection already exists through typed Ports and clients. Retain
that model. `Port<T>` need not be removed just to change syntax. The substantive
decision is to distinguish two requirements of the same Capability and allow
Host-permitted App selections to persist.

Give public dependencies stable consumer-local identities, independent of
internal Rust field renaming. Explicit names are the approved direction;
exact annotation syntax is open. A Capability identifies the interface, while
a requirement name identifies the consumer's use of it.

Keep fixed Host attachments. For selectable requirements, validate choices
against Host policy and contract compatibility. Preserve valid saved choices
across restart and unrelated installations. A missing or forbidden saved target
is an error, not an invitation to select another account. Ordinary required,
optional, and collection cardinalities retain their distinct meanings.
An optional absent binding is different from a bound provider that has failed.

Installation normally creates one default instance. Additional instances share
code but keep their own configuration, choices, work, and owned data. Display
renaming preserves identity. A disabled consumer retains its intent; invalid
dormant bindings are diagnosed and repaired before reactivation without becoming
an active dependency failure elsewhere. Structural input validation still applies.

Materialize selectable single/optional choices through install/configure,
including a deliberate optional absence. That absence survives new installations.
Startup and inspection never write. A fresh or explicitly migrated Root that
still needs selections saved must complete configuration before activation;
inspection explains the exact candidate. Existing Roots retain their accepted
startup behavior until explicit adoption. The file layout, transaction
representation, and version numbers are not fixed here. Preserve semantic
revisions, protect source edits separately, and prevent publication from mixing
inconsistent or unreviewed input.

## 7. Fault scope: accepted target with explicit adoption

Distinguish a dependency required by one consumer from an instance required
for the App's minimum useful operation. The Host should declare the latter;
its transitive required dependency closure determines App-critical failure
impact. A Plugin author
cannot make an authorization dependency dispensable by calling it optional.

A failed invocation does not itself prove the provider process is unhealthy.
Use bounded supervision for actual runtime failure, preserving stable bindings
across supported recreation. Do not automatically destroy all consumers, replay
in-flight calls, or substitute another provider. User disablement is intent,
not a failure that supervision should undo.

Keep ADR 0046 strict startup: all selected instances must activate before
readiness. Partial startup remains excluded. After readiness, the accepted
rule makes exhausted recovery terminal only for the Host's essential
instances and their required closure. Other failures remain visible as local
unavailability, without destroying consumer objects or rewriting bindings.

[ADR 0074](../adr/0074-scope-terminal-failure-to-host-essential-instances.md)
records the accepted amendment; the [companion](2026-09-04-plugin-fault-scope.md)
illustrates it. Implementation remains pending: current supervision can still
terminate the App when any consumer's required
provider exhausts recovery. Native process aborts cannot be contained by a
logical fault policy. SDK upgrades alone must not change this behavior.

## 8. Compatibility and implementation handoff

Preserve existing `Port` and lifecycle authoring during an explicit migration.
New syntax lowers into the same model, not a parallel runtime. Structural
requirement identity changes need versioned metadata, routing, and peer support;
unsupported readers reject them before business dispatch. Allocate format
versions against the actual release baseline, not numbers in an earlier sketch.

Legacy single requirements can preserve their exact provider where a new named
requirement maps unambiguously. Splitting, removing, or changing public identities
needs explicit migration. Package version, interface compatibility, configuration
compatibility, and data compatibility are separate checks. Structural comparison
cannot prove unchanged business meaning.

The [approved adoption and delivery design](2026-09-04-plugin-adoption-and-delivery.md)
now states the migration and sequencing decisions. Source syntax adoption,
named-dependency adoption, and Host fault-policy adoption are independent
decisions subject to actual peer/profile support. No SDK update changes account
selection, data compatibility, or terminal failure scope silently.

Design approval covers authoring/default semantics, the product SDK build
interface, explicit dependency migration, the first complete Request delivery
slice, and the separately recorded ADR 0074 fault policy. The first slice keeps
an official Rust implementation and a TS implementation against the same
contract, using Rust Native Store instances and Rust Process/TS Bun sync
implementations as proposed proof targets. Exact versions and availability must
be established in implementation specifications before code changes.

Write owner-local specifications for syntax, supported TS
declaration expressions, file/transaction formats, version boundaries, and
focused acceptance cases. These details must preserve this design's semantics;
new language engines or additional framework abstractions are not prerequisites.

Do not expand the design with mandatory resource categories, generated Inputs
types, a global service context, automatic state migration, or forced runtime
matrices. Implementation tickets must satisfy their specification and dependency
gates before runtime changes; this approval records no release or runtime evidence.
Earlier exploratory examples remain available in Git history.
