# Rust and TypeScript Plugin authoring

Status: **Design approved on 2026-09-04; the first Rust Process and TypeScript
Bun Request delivery completed on 2026-09-05.**
Date: 2026-09-04.

Delivery is tracked by [#695](https://github.com/LioRael/lenso/issues/695) and
specified by [#699](https://github.com/LioRael/lenso/issues/699). The shipped
scope does not claim TypeScript Stream/Event authoring, another JavaScript
engine, or automatic support for every language/runtime combination.

Read the [consolidated review](2026-09-04-plugin-usage-walkthrough.md) first.
This companion applies its construction and dependency rules to two languages.
Official implementations remain Rust-first. TypeScript must offer a complete
business-authoring path without requiring Rust source or an author-written wire
server. Additional language SDKs follow an actual consumer need.

## Retained foundations and current gaps

ADRs 0049 and 0050 already define portable values, generated bindings, structured
errors, and cross-language interaction semantics. ADR 0071 already permits one
Plugin Release with multiple exact implementations. These are foundations to
retain, not new features introduced by this proposal.

The inspected `lenso-bun-adapter/packages/lenso-bun-plugin` entrypoint already
provides `definePlugin({ providers })` and generated transport entrypoints. Its
current definition rejects Stream/Event descriptors. Generated protocol types
alone therefore do not establish full authoring or runtime support. The proposed
configuration, construction, dependency injection, and lifecycle composition
below extends that surface rather than asserting it exists today.

## Product conveniences use ordinary Capability declarations

`tools` is an Agent SDK concept, not a field in generic `definePlugin`. Retain
the existing generic `providers` entrypoint. The proposed Agent-owned
`tools([...])` helper lowers tool metadata and handlers to one standard
ToolProvider Capability declaration. Generic SDK code sees its contract and
instance binder, never Agent catalog, tool arguments, or execution policy.
The current Rust `lenso-agent-tool-sdk::tool_provider` already follows this
ownership pattern by generating ordinary `lenso::provides` implementations.

The TS helper and instance-aware declaration/binding behavior below are still
proposed. Product helpers use the same generated Capability machinery as an
ordinary handwritten Capability implementation. They do not add a second
Descriptor, lifecycle, permission system, discovery registry, or Plugin instance.
SDK grouping is not an extra installed Plugin. An actual Agent consumer still
needs the Host-resolved ToolProvider binding and applicable admission authority;
declaring tools does not automatically advertise them to every Agent.

Metadata lowering must use a documented generic build seam or generated binding
artifact. Generic tooling must not special-case the function name `tools` or
import Agent packages to extract declarations. The Agent build integration owns
tool-specific extraction, schema mapping, and dispatch generation; it supplies
standard contract metadata and later binds handlers to the constructed instance.
No handler or factory is executed to discover its metadata.

Do not use ambient TS module augmentation to make arbitrary product keys appear
on every generic definition. A product-specific facade could offer a top-level
`tools` convenience later, but it must be imported explicitly from that product
and lower through the same Capability entrypoint. No such facade is required
for the default authoring path.

## Smallest Agent Tool Plugin and progressive authoring

These are source-owner examples: each generates its own locked declaration.
Most imports and the ordinary scaffold's package identity are omitted. They are
proposed authoring syntax, not two variants claimed to share one Release.

```rust
#[plugin]
struct Uppercase;

#[lenso_agent_tool_sdk::tool_provider]
impl Uppercase {
    #[tool(name = "uppercase")]
    fn uppercase(&self, text: String) -> String {
        text.to_uppercase()
    }
}
```

```ts
export default definePlugin({
  providers: [tools([
    tool({ name: "uppercase", input: schema.string(), output: schema.string() },
      (text) => ({ ok: true, value: text.toUpperCase() })),
  ])],
});
```

Neither requires configuration, a dependency table, a constructor, a stop hook,
an explicit context, or asynchronous syntax. Rust's unit object is generated
per instance; the TS wrapper constructs one empty instance object. The SDK
owns registration and its cleanup. Package identity/version still come from
normal scaffolding; omitting them here does not replace packaging conventions.

In TS, `definePlugin` comes from the generic SDK; `tools` and `tool` come from
the Agent SDK. The Rust attribute is explicitly Agent-owned as well. The exact
released package interfaces are recorded by the Agent delivery rather than by
this illustrative snippet. A non-Agent Plugin provides its own Capability
without either helper.

For these Agent TS handlers, the fixed argument order is `(input, call, instance)`; trailing
unused arguments may be omitted. `call` is the invocation context; `instance`
is the already-constructed typed instance, not a lookup API. Optional spelling
changes may follow, but there is one handler convention rather than separate
simple, stateful, synchronous, and asynchronous registration systems.

The Rust synchronous return is lowered to success. TS retains the existing
typed result envelope; schema inference must not erase its domain/runtime
error distinction. A source-owned operation gets its schema from supported
typed declarations; implementing an existing operation instead references its
generated contract, without restating the schema. Unicode uppercasing is shown
only as local business logic: equivalent cross-language Release behavior would
also need an agreed Unicode contract and implementation evidence.

Adding configuration and dependencies alone still needs no custom constructor.
Rust auto-construction supplies annotated fields and valid default fields. The
TS default instance contains the admitted `config` and declared `dependencies`,
inferred from their declarations, so a handler may read
`instance.dependencies.source`. Optional facilities appear only when declared.
Inputs are resolved once for that instance, not discovered per invocation.

| Actual need | Author adds |
| --- | --- |
| Pure business operation | One operation declaration and handler. |
| Configuration or Capability clients | Typed declarations; automatic instance construction remains sufficient. |
| Private per-instance state | Ordinary Rust fields with valid defaults, or a TS `create` allocating fresh state. A factory may be synchronous. |
| Resource initialization that must be awaited | An asynchronous `create` returning a complete instance. |
| Explicit asynchronous resource cleanup | A `stop` hook; simple/default-owned fields do not require one. |

A counter is sufficient reason for a TS factory; factories are not restricted
to resource acquisition. A pure Plugin and a state-owning Plugin remain the same
model. Exported declarations are reusable definitions, not live singleton state.
Mutable module-level variables are never advertised as per-instance storage.
No new `state`, `setup`, `simplePlugin`, or automatic disposal vocabulary is
needed. Rust never fabricates an invalid default for a resource: if automatic
construction is not possible, require a constructor and identify the field.

## One behavior, two authoring forms

The example synchronizes one document from a source store to a destination
store, transforming it with a privately owned library engine. Loading its rules
requires asynchronous initialization; releasing the engine requires explicit
asynchronous cleanup. `TransformEngine` is an illustrative ordinary library,
not a new Lenso resource type, storage Capability, or universal data directory.

Both versions have exactly the same public configuration and dependencies:

| Declaration | Shared meaning |
| --- | --- |
| `document` | Nonempty string naming the document to copy. |
| `ruleset` | Nonempty string identifying the rules understood by the private engine. |
| `source` | Required Store Capability; stable public dependency identity. |
| `destination` | Required Store Capability; independent selection of the same Capability. |
| `sync_document` | Tool returning `updated` or `already_running`, with the same declared error contract; its name is inside ToolProvider, not a new Capability operation. |

For these two variants, `Config`, `SyncOutcome`, the generated Sync tool declaration,
and Store clients are generated projections of one locked contract source.
Their definitions are omitted to avoid two handwritten schema authorities.
The annotations, constructor mapping, tool binding, and helper names below are
illustrative; these blocks are not compilable examples of current packages.

### Rust

```rust
#[plugin]
struct DocumentSync {
    #[config]
    config: Config,
    #[dependency(id = "source")]
    source: StoreClient,
    #[dependency(id = "destination")]
    destination: StoreClient,
    engine: TransformEngine,
    running: Mutex<()>,
}

#[plugin_impl]
#[lenso_agent_tool_sdk::tool_provider]
impl DocumentSync {
    #[create]
    async fn create(
        config: Config,
        source: StoreClient,
        destination: StoreClient,
    ) -> Result<Self, InitError> {
        let engine = TransformEngine::load(&config.ruleset).await?;
        Ok(Self { config, source, destination, engine, running: Mutex::new(()) })
    }

    #[tool(contract = Sync::sync_document)]
    async fn sync(&self, call: &CallContext) -> Result<SyncOutcome, SyncError> {
        let Some(_guard) = self.running.try_lock() else {
            return Ok(SyncOutcome::AlreadyRunning);
        };
        let document = self.source.read(call, &self.config.document)
            .await.map_err(SyncError::from_read)?;
        let transformed = self.engine.transform(document);
        self.destination.put(call, &self.config.document, transformed)
            .await.map_err(SyncError::from_write)?;
        Ok(SyncOutcome::Updated)
    }

    #[stop]
    async fn stop(&self) -> Result<(), CleanupError> {
        self.engine.close().await
    }
}
```

The field annotations are the only dependency declarations. Constructor
parameters match annotated input fields by exact private field name and checked
type, never by argument order or type-only inference. The two Store clients
therefore need no repeated dependency attributes. Unknown names, mismatched
types, destructuring patterns, or private resource fields masquerading as inputs
are build errors with the valid input names. An arbitrary ordinary helper
function remains available behind this one generated construction entrypoint.

Private field renames require the corresponding constructor parameter rename
but retain the explicit public dependency ID. A mismatch fails rather than
silently changing accounts. Do not add a second alias/selector syntax before a
real case needs it. No generated public `DocumentSyncInputs` type or
author-provided `Clone` is necessary. This replaces the earlier repeated
constructor-selector sketch; final macro spelling remains open.

### TypeScript

```ts
export default definePlugin({
  config: Sync.configSchema,
  dependencies: {
    source: dependency(Store),
    destination: dependency(Store),
  },

  async create({ config, dependencies: { source, destination } }) {
    const engine = await TransformEngine.load(config.ruleset);
    return { config, source, destination, engine, running: false };
  },

  providers: [tools([
    tool(Sync.syncDocument, async (_input, call, instance) => {
      if (instance.running) return Sync.ok("already_running");
      instance.running = true;
      try {
        const read = await instance.source.read({ key: instance.config.document }, call);
        if (!read.ok) return Sync.fromReadError(read.error);

        const document = instance.engine.transform(read.value);
        const write = await instance.destination.put({
          key: instance.config.document, document,
        }, call);
        if (!write.ok) return Sync.fromWriteError(write.error);
        return Sync.ok("updated");
      } finally {
        instance.running = false;
      }
    }),
  ])],

  async stop(instance) {
    await instance.engine.close();
  },
});
```

`create` receives an inferred, closed set of inputs rather than a service
locator. The declaration keys `source` and `destination` are public identities;
authors may destructure them into different private variable names. Nesting
the declared dependencies keeps their names independent of configuration and
context inputs. The object shape is inferred; authors do not maintain an extra
Inputs interface or query a mutable dependency registry.

Capability declarations and the stop function stay outside construction. A custom
`create` returns the complete instance visible to handlers; its inferred return
type replaces the automatic input-object shape, with no hidden merge or second
state bag. The factory still receives the same typed inputs. Returning a
configuration/client field simply retains a reference to that input.

An Agent tool name appears once, in its source declaration or generated tool
reference. Duplicate tool registrations fail product build validation; duplicate
Capability declarations fail generic assembly. The generated wrapper
connects each declaration to its instance only
after successful construction, and opens external admission only at readiness.
This supersedes the earlier factory-returned registrations sketch and avoids
requiring build tooling to inspect arbitrary factory bodies.

The boolean guard relies on one JavaScript execution context per object and is
set before the first await. It does not protect another instance or worker.
The Rust mutex likewise protects only that object. Neither example serializes
the entire Plugin or introduces shared mutable state across execution lanes.

Generated TS clients retain discriminated success/domain/runtime results. The
illustrative product helpers `fromReadError` and `fromWriteError` explicitly map
Store domain errors into Sync domain errors while preserving runtime failures
and unknown domain details in the declared representation. Their Rust counterparts
do the same. Do not silently relabel every dependency error as a Sync business
error. Unexpected JS exceptions are sanitized runtime failures, not arbitrary
serialized Error objects or declared domain errors. An optional throwing wrapper
can be considered later; it is not required by this design.

The engine's transform is synchronous and infallible after loading in this
example. Both stores use the same document/replace semantics; competing external
writers require an explicit conflict policy. A failed or cancelled write may
have taken effect. Neither the lock nor error mapping promises atomic copy,
exactly-once execution, or data rollback.

## Contract source and declaration discovery

The canonical Descriptor/Schema is generated from one author-maintained source,
then locked and used to generate bindings. Each contract has one source owner;
other implementations consume its projections. Rust source can own that source,
but a TS-only Plugin can instead own a typed schema declaration that generates
the same canonical artifacts and infers its TS configuration type. It does not
need Rust, a handwritten JSON manifest, or a parallel handwritten interface.

For example, the TS-owned equivalent configuration source could be:

```ts
const config = schema.object({
  document: schema.string().minLength(1),
  ruleset: schema.string().minLength(1),
});
// definePlugin({ config, ... }); the Config type is inferred from this value.
```

This is an alternative source owner, not a second schema maintained beside
`Sync.configSchema`. Erased TS interfaces alone cannot supply runtime validation.
The schema builder's supported subset must generate the existing portable value
profile without introducing a second type system or schema dialect.

Build tooling extracts declarations and operation contracts without calling
`create`, invoking handlers, or opening resources. The product helper's standard
Capability declaration carries the tool metadata derived by its owning SDK. Resource
initialization, executable top-level side effects, and business-dependent
conditional exports cannot be required for metadata discovery. If the supported
build subset cannot extract a declaration, report an authoring error; do not
execute arbitrary Plugin code as an installation probe. Runtime bindings must
match the locked declarations before readiness, even when written in TS.

The [declaration pipeline](2026-09-04-plugin-declaration-pipeline.md) makes this
concrete: a language frontend passes bounded declarations to normal SDK build
entrypoints, which produce standard metadata and binding code. No import of
the application module is used for metadata discovery. SDK build code remains
a trusted executable build dependency; offline installation loads neither it
nor the Plugin. This differs from the current Bun describe-script path.

## Shared behavior, language-specific implementation

| Concern | Shared rule |
| --- | --- |
| Construction | Resolve config/dependencies first. Run custom construction in the activation phase after required providers are callable; return one complete object. Ordinary Plugins without custom resources need no custom factory. |
| Startup failure | Constructor rejection fails activation under strict startup. Until it returns, the factory owns cleanup of acquired resources, including partial initialization. Helpers must close resources when their own initialization fails. |
| Ownership transfer | After factory return, runtime owns the object's lifecycle. Late return after cancellation goes directly to cleanup, never activation. Attempt its stop hook at most once when safe and within budget, including later startup failure. |
| Shutdown | Close admission and triggers; drain/cancel managed calls before normal cleanup within the shared deadline. A cancellation signal alone does not prove stopped resource access; skip unsafe concurrent stop-hook execution and apply actual Adapter escalation. |
| Cancellation | Carry the inherited call deadline and cancellation through dependency calls. Rust uses scoped cancellation facilities; TS exposes a signal and remaining budget. Promise settlement or dropped futures alone do not prove external work stopped. |
| Private resources | Ordinary language libraries remain available under the actual execution profile. Construction receives bounded cancellation through an optional lifecycle context when needed; no arbitrary resource lookup is added. |
| State | One object per instance; closures and Rust fields have the same ownership duration. Restart constructs fresh state without implicit replay or automatic durable-state migration. |
| Tasks | SDK-managed work belongs to the instance and cannot outlive its managed scope. Raw timers, detached promises, threads, or external workers do not automatically become managed. |
| Fault scope | The Host policy is language-independent. Accepted ADR 0074 requires supported implementation and explicit adoption; a new SDK does not implicitly activate it. |

Neither finalizers nor Rust destructors replace bounded asynchronous shutdown.
No mandatory `create` plus `onEnable` pair is added. Ordinary configuration and
clients are explicit inputs, with a small optional instance context and separate
call context as described by the consolidated review.

The [cancellation and cleanup review](2026-09-04-plugin-cancellation-and-cleanup.md)
specifies constructor races, terminal request outcomes, execution capacity held
by unfinished work, and cleanup with remaining budget. It separates portable
cooperation from physical termination and does not promise that TS `finally`
or Rust async cleanup always runs after timeout.

## Language and execution support are different

Rust-native calls may remain typed and direct. Portable boundaries retain the
existing value profile, including safe integers, explicit wide integers and
bytes, missing versus null, and open domain errors. Do not force every native
call through JSON to make SDKs look alike.

For a published implementation, packaging declares the actual target, execution
class, and required supported interactions/facilities. Tools should explain an
unsupported declaration before packaging or activation; host-specific execution
failures remain runtime outcomes. Do not promise static detection of every
dynamic library or resource requirement.

A TS Plugin using Bun-specific libraries need not run in QuickJS, and a Rust
Plugin using native libraries need not compile to Wasm. Those are valid Plugins
with narrower implementation sets. Common source is not proof of portability.
The engine in these sketches has no implied Wasm or QuickJS implementation.
Process isolation is not filesystem/network sandboxing.

For the same Release, Rust and TS variants must satisfy the same public contract
and behavior under ADR 0071; different defaults, requirements, or data meanings
cannot hide behind implementation selection. Independent Rust and TS Plugins
only need compatible Capabilities to cooperate. Neither case requires matching
languages, runtime fallback, or a full language-by-runtime product matrix.

## Recommended adoption boundary

Keep existing Port/client contracts and TS result envelopes. Extend authoring
around them with construction, named inputs, and lifecycle ownership. The
initial two-language design covers Request behavior in detail; first-class
Stream, Event, and managed scheduling paths remain explicit delivery work and
must not be advertised as supported from generated types alone. Their accepted
interaction semantics remain the common target, not a Request-only ceiling.

Protocols owns the canonical value/error projections and generated clients;
language SDK owners implement their authoring and lifecycle forms; Runtime
Drivers/Execution Adapters supply actual execution; product SDKs own tools and
routes. Core does not acquire TS package resolution, Rust reflection, a dynamic
service registry, or a language-specific schema system.

Implementation specifications must preserve the approved input identity,
constructor failure, cleanup ownership, declaration extraction, and error
semantics. Executable acceptance should use the same contract cases in
Rust and TS, including mixed-language dependency calls and unsupported-profile
rejection. This document establishes no compilation, runtime conformance, or
performance result.
