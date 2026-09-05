# Plugin authoring, dependency selection, and lifecycle

Status: **Design approved; its first Request authoring slice and ADR 0073/0074
contracts shipped on 2026-09-05. Broader directions remain explicitly deferred.**
Date: 2026-09-04.
Design approval: 2026-09-04, following repository-owner review.

For the current candidate, read the
[consolidated authoring review](2026-09-04-plugin-usage-walkthrough.md).
This document retains the earlier approved direction and exploratory sketches;
its syntax examples and open questions are not final authoring requirements.
The delivery record in [#695](https://github.com/LioRael/lenso/issues/695) is
authoritative for shipped support; later sections that discuss pre-implementation
state remain historical design context.
The consolidated baseline records later corrections and was approved on
2026-09-04. ADRs 0073 and 0074 record its accepted dependency and fault-policy
changes; the exploratory examples below remain historical rather than final APIs.

This proposal makes ordinary Plugin development smaller while retaining a
composable, multi-language runtime. Official Plugins remain Rust-first. A
simple function can grow into a stateful Plugin without changing its identity,
configuration model, or distribution model.

The authoring direction, design constraints, and migration approach in this
document have been approved. API syntax and the remaining decisions listed
below still need specification before the affected implementation work. This
approval does not claim delivery, change existing compatibility guarantees, or
supersede accepted ADRs. Changes to normative contracts require follow-up ADRs.

## Design constraints

- Keep one Plugin model for embedded, bundled, and independently installed
  behavior. Keep Capability contracts for collaboration between Plugins.
- Prefer Rust for official implementations. Give Rust and TypeScript idiomatic
  authoring interfaces rather than making either language imitate a wire API.
- Keep business logic, private data, and ordinary library calls inside a
  cohesive Plugin. Multiple tools, routes, and tasks do not require multiple
  Plugins.
- Derive repeated facts from source. Configuration, dependency, and business
  choices remain explicit where they cannot be inferred safely.
- Expose reliable installation, configuration, disablement, and updates.
  Online replacement is an optional Host ability, not a requirement for every
  Plugin or a substitute for data compatibility.
- Keep scheduling in Drivers, execution and isolation in Adapters, and business
  policy in its owning Plugin or Host. This proposal adds no HTTP, database,
  account, or migration policy to Kernel.

## What people need to understand

An App owner installs and configures Plugins, selects accounts or other named
dependencies when needed, and sees which features are available. A Plugin
author writes business functions, configuration types, explicit dependencies,
and necessary state and recovery logic. Capability is the typed interface used
when another Plugin must consume or provide a role.

Framework maintainers still need precise internal vocabulary. Ordinary usage
does not require managing those internal values:

| Internal mechanism | Ordinary interface |
| --- | --- |
| Port and binding | A named, typed dependency |
| Slot | Register a tool, command, route, listener, or declared decision handler |
| Plan and App Composition | Check dependencies and start the App |
| Generation and Plan Transition | Start, stop, update, and report the outcome |
| Execution Class and Adapter | Build for a supported target; inspect compatibility |
| Reconciler and Desired State | Apply installation or configuration changes |

This is progressive disclosure, not deletion of correctness mechanisms. Errors
first identify the Plugin, operation, cause, and remedy; advanced details expose
the selected contracts, bindings, runtime, and lifecycle state. Do not rename an
internal value and make the author manage it under a friendlier name.

## Three authoring examples

All Rust below is **illustrative proposed syntax**, not a compile-ready SDK
example. Attribute names, helper types, and method signatures are review
subjects. Each sketch represents one Plugin package with ordinary Cargo
metadata; SDK lowering supplies registration and runtime glue.

### 1. A small Tool

```rust
#[plugin]
struct TextTools;

impl TextTools {
    #[tool(description = "Convert text to uppercase")]
    fn uppercase(text: String) -> String {
        text.to_uppercase()
    }
}
```

The SDK derives the Tool catalog, input schema, request decoding, validation,
result encoding, and registration from this source. Adding a second Tool adds a
method. Business validation remains expressible in types or domain code; the
SDK must not require a second handwritten JSON validator. Generated size limits
must have consistent semantics across languages, including Unicode and byte
limits.

The richer authoring form below expands the same Plugin. Function-level
conveniences must not become a separate package or lifecycle system.

### 2. A stateful synchronization Plugin

```rust
#[plugin]
struct GitHubSync {
    #[config]
    config: SyncConfig,
    #[dependency(label = "Source account")]
    source: GitHubClient,
    #[resource]
    store: CursorStore,
}

impl GitHubSync {
    #[start]
    async fn start(&mut self, resources: &Resources) -> Result<(), SyncError> {
        self.store = CursorStore::open(resources.data_dir()).await?;
        self.store.check_version().await?;
        Ok(())
    }

    #[command(name = "sync")]
    #[schedule(config = "interval", overlap = "skip")]
    async fn sync(&self, cancel: &Cancellation) -> Result<SyncReport, SyncError> {
        let cursor = self.store.load_cursor().await?;
        let page = self.source.fetch_page(cursor, cancel).await?;
        let report = self.store.apply_page_and_cursor(page).await?;
        Ok(report)
    }
}
```

`CursorStore` is owner-private implementation, not another mandatory Plugin or
Kernel state service. The sketch leaves resource construction syntax open:
startup must produce initialized fields without requiring invalid placeholders.
Opening a data directory assumes an admitted filesystem implementation; a Wasm
target must instead use a supported storage interface or be rejected at build
or admission. No ambient filesystem permission is implied.

The account Plugin owns authentication and authenticated GitHub access. The sync
Plugin owns synchronization rules, cursor meaning, page application, and its
transaction. The source dependency is explicit and Host-admitted. A Plugin that
uses HTTP directly declares that dependency and an appropriate authentication
interface instead; secrets do not enter generated descriptors or ordinary logs.

The scheduled and manual entrypoints share one execution rule: `overlap = skip`
means a second run does not start while the first is active, and a manual caller
receives a visible busy/skipped outcome. Managed triggers wait for readiness and
stop when disablement begins. They do not start external work during staging.
The scheduling shorthand requires a real supported scheduling implementation;
the existing one-shot Jobs queue alone does not establish recurring execution
or durable delivery. Persistence across restart and delivery guarantees need
their own explicit contract.

Local page application and cursor advancement commit atomically. Remote effects
still require idempotency and recovery logic. A convenient state wrapper must
not imply a transaction across external APIs or automatic exactly-once work.
Tests can substitute storage and remote responses through the same interfaces;
production must never silently fall back to ephemeral storage.

### 3. A Plugin participating in an execution decision

```rust
#[plugin]
struct ProtectedToolApproval {
    #[config]
    config: ApprovalConfig,
    #[dependency]
    approvals: ApprovalClient,
}

impl ProtectedToolApproval {
    #[decision("agent.tool.before_execute")]
    async fn review(
        &self,
        request: &ToolIntent,
        cancel: &Cancellation,
    ) -> Result<ToolDecision, ApprovalError> {
        if self.config.requires_approval(request) {
            return self.approvals.request(request, cancel).await;
        }
        Ok(ToolDecision::Continue)
    }
}
```

The Agent Host owns this named extension interface and the allowed handlers,
ordering, deadline, and failure behavior. The author does not gain a global
interceptor over arbitrary Capability calls. The proposed decision interface is
not a notification Event: the caller must await its outcome before executing.

For this approval example, deny is terminal, timeout or failure blocks the
affected operation, and `Continue` never overrides required resource
authorization. An input-transforming interface must separately declare which
fields may change and when validation and authorization run again. It must not
reuse approval of one action to execute a different action.

The SDK may lower such handlers into existing Request Capabilities and
Host-ordered attachments. Selecting that lowering does not establish support
for a universal mutable middleware chain.

## Capability and extension rules

Use ordinary Rust calls within a Plugin and Cargo dependencies for ordinary
libraries. Use a Capability when a dependency is an App-selected Plugin or a
published interface consumed by other Plugins. Native bindings may dispatch
typed calls directly; cross-runtime bindings use generated transport code.

Keep contract identity separate from package layout. Several related contracts
may ship in one SDK package; a private helper does not need an independently
versioned contract or repository. Public contracts retain explicit compatible
evolution and their existing owners. TS-only authors should not need to write
Rust to define a portable interface.

| Extension | Required semantics |
| --- | --- |
| Add an entrypoint | Name conflicts, visibility, registration lifetime |
| Subscribe to a notification | Admission, ordering, delivery and retry guarantees; volatile delivery is the default only where the contract says so |
| Participate in a decision | Stable ordering, terminal outcomes, deadlines, permitted transformations, and failure behavior |
| Replace an implementation | Contract compatibility, explicit selection, state compatibility, and activation rules |

Official Hosts should offer useful default implementations for common admitted
dependencies. Logging, typed configuration, cancellation, and managed resource
lifetimes should not require manual infrastructure assembly. Product services
remain explicitly declared dependencies. No unrestricted service locator or
access to another Plugin's private tables is introduced.

## Stable dependency choices

A sync Plugin can select a personal or work account. Another Plugin can have
both a source database and a destination database implementing the same
Capability. Choosing one must not require disabling the other.

Proposed selection behavior:

1. Validate a saved explicit or previously accepted choice against current Host
   policy and contract compatibility. Keep it if valid; report its failure if
   not. Do not fall back to a different account or instance.
2. For an unselected dependency, use a valid Host default or select the only
   legal candidate. With several candidates, ask for a named business choice.
3. Commit newly accepted automatic choices with the successful App change.
   Pure resolution must not write files; failed or cancelled changes leave the
   prior choices and running App unchanged.
4. Later installations do not change existing choices. Choosing a new target is
   an explicit App change, validated before taking effect.

Host-required fixed attachments and authority ceilings remain authoritative.
User choices operate only within the Host's allowed candidates. A saved choice
cannot grant a new permission or bypass operation-level authorization.

Persist identity, not just a display label. A dependency field needs a stable
identity within the consumer, including two fields with the same Capability.
Renaming that identity requires a migration or an explicit new choice. This is
a real metadata/resolution design change: the inspected resolver indexes
attachments by consumer identity and Capability, not a distinct author field.

Choice provenance may record whether a default or user selected it, but there
must be one durable authority. Do not add a competing binding file beside
Plugin Root intent. The storage syntax, export behavior, reset-to-default
behavior, and treatment of existing ambiguous installations remain open.

## State, disablement, and updates

| Action | Proposed ordinary behavior |
| --- | --- |
| Enable | Validate configuration and dependencies; prepare before accepting work |
| Disable | Stop new triggers, allow bounded completion, request cancellation, and report outstanding work |
| Change configuration | Validate the candidate; apply through a supported safe transition or restart; retain the old accepted state on failure |
| Update | Check compatibility, stop old writers when required, perform the permitted migration, then start the new version |
| Uninstall | Remove code and entrypoints; retain configuration and data by default; expose data deletion separately |

Only tasks and resources created through supported lifecycle-aware SDK paths
can be automatically tracked. Arbitrary spawned threads or external work are
not magically cancellable. Long operations must define safe checkpoints.

Use controlled restart as the baseline for stateful updates. Validation can run
while the old version serves, but a staging candidate must not start business
tasks or irreversibly migrate shared data. By default, one stateful instance has
one active writer across an update. Online overlap requires an explicitly
supported state protocol.

The state owner declares readable versions and migrations. Installing old code
does not roll back data or external effects. Restore the previous implementation
only when state remains compatible; otherwise keep the affected feature stopped
with a recovery action. Destructive or irreversible migrations require an
explicit operator decision before execution. Backup, restore, shared data
ownership, retention, and interrupted-migration recovery must be specified before
promising a generic state-management API.

## Failure scope

Proposed product policy: the Host defines its minimum required functions and
which additional functions may be unavailable. A Plugin cannot unilaterally
make itself critical or weaken its consumers' required checks.

- Installation/update rejection preserves the active App.
- An unavailable dependency blocks functions that require it. Missing required
  authorization never becomes an implicit allow.
- An additional function may fail independently only when Host policy permits
  it and the remaining App has a complete valid dependency graph.
- Transient failures may receive bounded retries with backoff. Invalid
  configuration, incompatible versions, and failed migrations require repair.
- Diagnostics report the unavailable feature and a remedy, not endless restarts
  or silent provider substitution.

This would change initial-boot policy. ADR 0046 currently requires all selected
instances to prepare and activate before readiness. It is not enough to ignore
an activation error. A future Host path would need to clean up the failed
candidate, explicitly select an allowed reduced set, and resolve a valid Plan;
irreversible staging effects cannot be undone by graph reduction. Runtime
failure handling and the effect on active dependents need separate specification.
Existing all-or-nothing boot remains the default until that design is accepted.

Error containment also depends on execution. Linked Rust plugins share a process;
they cannot promise survival of every native crash. A process boundary or Wasm
can provide stronger containment subject to its implementation and limits.
Business-error handling must not be advertised as a hostile-code sandbox.

## Language, execution, and compatibility

Official Plugins default to Rust. Embedded official behavior can use native
linked execution; independently installed packages declare the runtime targets
they actually support. A scaffold should not require Wasm plus Process output
for every Rust project. The exact single-target default remains a Host/product
decision to settle during review.

Preserve ADR 0071: every implementation published in one Release must implement
the same Contract, including configuration, errors, cancellation, and state
semantics. Optional multiple targets do not permit weaker semantics under the
same identity. A target lacking a dependency or interaction is rejected before
readiness. Alternative implementation selection never occurs as failure fallback.

Compatibility should be understandable at three levels: the App owner selects
a Plugin release, the author declares compatible interface requirements, and
the tooling locks exact resolved versions. Language SDKs generate equivalent
wire validation. Startup timing, scheduling, and state behavior remain semantic
contracts that schema compatibility alone cannot prove.

No support duration or compatibility window is promised by this design. A future
policy must define the supported Host/SDK range, deprecation period, data
migration obligations, and how old packages fail with actionable messages.

## Current baseline and actual design changes

The core baseline was inspected at `25dc4e6177033023e2ab9f857b022aa487b1c542`
(2026-09-04). Sibling implementation observations are pinned source snapshots,
not claims about the newest published packages or executable end-to-end proof.

| Area | Accepted or observed today | Proposed change |
| --- | --- | --- |
| Ordinary user concepts | [Plugin Root contract](../architecture/plugin-root-resolution.md) already makes Plan and Generation derived facts | Fulfill that separation in SDKs, CLI errors, and ordinary documentation |
| Source-derived authoring | [ADR 0066](../adr/0066-derive-module-descriptors-and-plans-from-source.md) already calls for source-derived metadata | Finish a coherent path for small, stateful, and decision-handling Plugins; settle syntax without a parallel authoring model |
| TS Tool scaffold | [Inspected scaffold](https://github.com/LioRael/lenso-cli/blob/f293c9636e826c1e2ca6dfa104af2f0d203263ed/src/plugin/scaffold.rs#L370) repeats catalog, schema, parsing, and validation | Move generated protocol work behind typed business entrypoints |
| Portable CLI descriptor path | [Inspected parser](https://github.com/LioRael/lenso-cli/blob/f293c9636e826c1e2ca6dfa104af2f0d203263ed/src/plugin.rs#L725) admits request-only V1 with exactly one provided Capability | Support complete declared contracts through the actual package/Host path |
| Wasm mechanisms | [Adapter source](https://github.com/LioRael/lenso-runtime-rust/blob/96d034ad72638b2ada5844cb9766a1fd8672fcf4/crates/lenso-wasm-component-adapter/src/lib.rs#L60) has Host Request and Stream imports and no ambient WASI | Expose supported mechanisms through authoring and admission; do not confuse Adapter support with a complete Plugin workflow |
| Web distribution | [CLI documentation](https://github.com/LioRael/lenso-cli/blob/f293c9636e826c1e2ca6dfa104af2f0d203263ed/README.md#L45) describes linked Web authoring rather than portable Tool packaging | Define external entrypoint support explicitly before promising installable Web parity |
| Dependency selection | [Resolver](../../crates/lenso-app-plan/src/authoring/plugin_root/resolution.rs) supports unique matching and Host-private attachments; public binding edits remain forbidden | Add permitted named-instance choices, stable persistence, and distinct dependency-field identities; amend ADR 0070/CONTEXT only after acceptance |
| Initial readiness | [ADR 0046](../adr/0046-use-staged-all-or-nothing-app-activation.md) requires all selected instances to start | Review opt-in Host policy for a reduced valid feature set; retain strict Kernel readiness |
| State ownership | [ADR 0041](../adr/0041-keep-persistence-owned-by-stateful-modules.md) keeps persistence and recovery with the state owner | Specify user-visible stop, migration, recovery, and retention guarantees without adding a universal Kernel database |
| Multiple implementations | [ADR 0071](../adr/0071-publish-one-plugin-contract-with-multiple-implementations.md) permits equivalent implementations; [CLI defaults](https://github.com/LioRael/lenso-cli/blob/f293c9636e826c1e2ca6dfa104af2f0d203263ed/src/plugin.rs#L55) choose multi-output | Make multi-output an explicit choice while preserving Contract equivalence |

## Keep, simplify, remove, and defer

**Keep:** Rust-first official behavior; one Plugin identity; typed cross-Plugin
interfaces; explicit authority; immutable resolved execution inputs; managed
lifetimes; independent execution targets with honest support boundaries.

**Simplify:** common authoring syntax, generated validation and errors, product
extension registration, source-level diagnostics, default dependency setup,
and SDK/package distribution. A large internal implementation is acceptable
when its small interface reliably owns that complexity.

**Remove after replacement:** duplicate author-maintained schemas and dispatch,
manual registration already derivable from source, ordinary workflows requiring
Plan/Generation knowledge, and an unconditional dual-output default. Internal
structures should be merged only when they duplicate authority rather than
protect different invariants; this design does not identify a specific redundant
Kernel state machine for deletion.

**Defer:** more execution runtimes, automatic cross-runtime parity, universal
hot replacement, and broad WASI expansion without a concrete consumer. Also
defer exact macro names and a generic `State<T>` facade until initialization,
durability, and migration rules are coherent.

## Migration and ownership

1. Specify the remaining decisions below against the approved examples. Record
   accepted changes to ADR 0070, ADR 0046, or other contracts through new ADRs, without
   rewriting historical decisions. This design alone changes no normative rule.
2. In `lenso-runtime-rust`, `lenso-protocols`, and product SDK owners, deepen the
   existing authoring path and share lowering rather than add another runtime
   protocol. In `lenso-bun-adapter`, expose the same business contracts through
   idiomatic TS. Keep existing Plugin IDs and compatible behavior intact.
3. After specifying choice identity and persistence, evolve metadata and
   resolution in `lenso-app-plan`; implement persistence and configuration UI/CLI in the
   authoring and Host owners. Existing valid Apps should retain their effective
   dependencies through an explicit migration transaction. Previously ambiguous
   Apps still need a choice; do not invent one from installation order.
4. In `lenso-cli` and product Hosts, align scaffold, development, package,
   installation, and diagnostics. Existing multi-output projects continue to
   work; change new-project defaults separately. Preserve configuration and
   state formats or provide a reviewed migration.
5. Introduce new lifecycle or decision contracts with their actual product
   owners. Reuse existing HTTP, Jobs, Auth, and storage implementations where
   appropriate; no duplicate technical Plugins solely to populate the diagram.
6. Retire superseded authoring paths only after replacements cover their real
   users. Keep one shared lowering during migration and make any compatibility
   break explicit; avoid an indefinite legacy subsystem.

Design acceptance is complete. The first Request slice was implemented and
released separately through #695. The illustrative examples still describe the
approved direction rather than serving as versioned API reference or benchmark.

## Original follow-up decisions

The first delivery resolved construction, dependency choice, supported failure
scope, and its Rust Process/TS Bun packaging boundary. Background work, decision
handlers, general state migration, additional targets, and long-term support
policy remain demand-driven follow-ups rather than incomplete first-slice work.

| Decision | Approved direction | Remaining question |
| --- | --- | --- |
| Authoring shape | Typed Plugin with function-level shorthand | How are private resources constructed and cleaned up without hidden invalid state? |
| Dependency choice | Stable named selections within Host policy | Where does choice intent live, how is it exported/reset, and how do dependency-field identities lower into the Plan? |
| Background work | Managed tasks with explicit overlap and delivery behavior | Which scheduling behaviors are ephemeral, durable, or product-specific? |
| Decision handlers | Named Host extension contracts with deterministic order | What are the first supported decisions, transformation rules, and compatibility obligations? |
| Stateful updates | Controlled restart and explicit data compatibility | Which migrations require backup, what is crash recovery, and when is rollback eligible? |
| Failure scope | Strict boot by default; consider permitted reduced functionality | How can failed preparation be cleaned up safely before forming a reduced valid App? |
| Packaging and support | Rust-first, targets chosen deliberately | Which external target defaults serve each Host, and what compatibility window can we sustain? |

Follow-up specifications must preserve how the same Plugin grows from a command
into configured, multi-account, stateful, scheduled behavior and then survives
a compatible update. Dependencies, side effects, and recovery must remain
understandable without requiring ordinary authors to construct a Generation or
Plan. A bounded prototype may resolve an implementation uncertainty after the
relevant behavior has been specified; it is not a substitute for that decision.
