# Three Plugin usage walkthroughs

Status: **Discussion draft. No implementation approval.**
Date: 2026-09-04.

Read this before deciding the constructor interface, dependency naming, or
storage representation in [Issue #695](https://github.com/LioRael/lenso/issues/695)
and [proposed ADR 0073](../adr/0073-name-and-persist-plugin-dependencies.md).
Those documents describe an earlier candidate. This walkthrough reopens its
generated Inputs type, mandatory explicit dependency IDs, per-instance choice
files, and implicit startup writes. The approved overall direction remains
[Rust-first Plugin authoring](2026-09-04-plugin-authoring-and-lifecycle.md).

All commands, Rust attributes, helper types, and output below are **proposed
usage**, not a runnable tutorial or evidence of current SDK support. This is a
paper walkthrough: it examines what a person must do and understand. It is not
a runtime validation exercise or an implementation checklist.

## The shared path

The developer needs to know which App will consume the Plugin. An Agent tool,
a Web endpoint, and a new reusable Capability have different consumers. A
starter can select the appropriate SDK and sample; it cannot make an arbitrary
function useful in every Host without a consumer contract.

Proposed command surface for an Agent tool:

```sh
lenso plugin new company.text-tools --template agent-tool
cd company.text-tools
lenso plugin dev --root /absolute/path/dev-app
lenso plugin pack
lenso plugins add /absolute/path/company.text-tools-0.1.0.lenso-plugin \
  --root /absolute/path/app
```

`new` creates ordinary Cargo source, dependencies, and generated defaults.
`dev` connects to an explicitly chosen development App, builds, reports errors,
and reloads through that App's supported restart path. A failed build keeps the
last working development instance; a failure after stopping it reports that it
is stopped. It never implies zero-downtime replacement or writes into an
unrelated production App. The first invocation must identify the development
App and its data directory so it is not mistaken for disposable state.

`pack` produces the supported artifact and displays its runtime and compatible
Host requirements. `add` validates the package, displays proposed instance
choices and configuration, and applies them through the App's supported
management path. A required choice can be supplied noninteractively; ambiguity
must fail in a script rather than waiting indefinitely for input.

The default installable Rust starter proposes Process output. Official Plugins
linked into an App use native Rust. A starter does not ask every author to
design an artifact matrix. A template selects an example and SDK, not a new
Plugin kind: changing a stateless Plugin into a stateful one preserves its
package and installed identity.

## 1. A small text tool

The author edits this business source, with imports supplied by the starter:

```rust
#[lenso::plugin]
struct TextTools;

#[agent::tools]
impl TextTools {
    #[tool(description = "Convert text to uppercase")]
    fn uppercase(text: String) -> String {
        text.to_uppercase()
    }
}
```

`agent` denotes the product-owned tool SDK, not a new universal Kernel module.
The supplied Tool Capability is generated from that SDK. The author does not
define a Capability or transport merely to add this tool. A second operation
adds a method to the same Plugin. The struct keeps one obvious path to adding
state later; a standalone-function shorthand is optional and need not be a
second first-page authoring form.

| Stage | What the person does | What should happen |
| --- | --- | --- |
| Create | Choose the Agent tool starter and a package ID | Receive source and a working example consumer; no Slot or Descriptor editing |
| Write | Change the function and description | Generate its tool schema and dispatch from the same source |
| Try | Run development mode and invoke `uppercase` in the chosen App | See input, output, and errors associated with this tool |
| Install | Add the packed release | See runtime compatibility; no dependency or state questions for this Plugin |
| Update | Install the next version through the supported App action | Preserve Plugin identity; report whether restart is needed |
| Remove | Remove the Plugin | Its tool disappears; no business code remains in the Host |

The source adds two meaningful framework ideas: a Plugin groups behavior, and
a tool exposes a function to an Agent. No initialization hook, config schema,
resource field, dependency selection file, Generation, or Plan belongs in this
starter. Cargo/package metadata still exists; the starter derives mechanical
facts instead of pretending there are no packaging inputs.

## 2. A counter that survives restart

Grow the tool Plugin into one that owns a small durable counter. Keep storage
as an ordinary Rust library inside the Plugin:

```rust
#[lenso::plugin]
struct Counter {
    store: CounterStore,
}

impl Counter {
    #[lenso::create]
    async fn create(data: lenso::DataDir) -> Result<Self, CounterError> {
        Ok(Self { store: CounterStore::open(data.path()).await? })
    }
}

#[agent::tools]
impl Counter {
    #[tool(description = "Increase the saved counter")]
    async fn increment(&self) -> Result<u64, CounterError> {
        self.store.increment_and_commit().await
    }
}
```

`CounterStore` and its error handling are business implementation, not hidden
framework code. `DataDir` is a candidate narrow SDK handle for an admitted,
Instance-owned local directory. It supplies a path, not a service registry,
database, automatic transactions, or access outside the runtime's permissions.
Its path is stable across compatible code updates, isolated by App and Instance,
and independent of the shell's current directory. Exact type spelling remains
open; making every user invent a unique storage path is the less usable option.

The proposed constructor returns a complete Rust value. Its explicitly typed
parameters receive declared configuration/dependencies or narrow SDK inputs.
There is no generated `CounterInputs` type to learn. This requires an explicit
parameter mapping convention and useful diagnostics; it is not arbitrary
lookup by type. Unknown parameters, duplicate inputs, and ambiguous field
matches fail at compile time. An optional stop hook can flush a library that
needs it; successful operations already commit their durable effects.

| Stage | What the person does | What should happen |
| --- | --- | --- |
| Create/write | Add a storage library, private field, and constructor | Keep the same Plugin; no storage Plugin or invalid default field required |
| Try/restart | Increment, stop, and start the development App | Retain the counter in the displayed development data directory |
| Add another instance | Create a separately named instance | Get separate data by default; never reuse the first instance's file accidentally |
| Install | Add the package | Allocate its persistent directory; show a useful path/access error if unavailable |
| Update | Install compatible code and restart | Stop the old writer before opening the new one; retain the same data |
| Failed update | Inspect the error | Report stopped if old execution already ended; restarting old code requires compatible data |
| Remove/reinstall | Remove code, later reinstall the same identity | Retain data by default and report that fact; deletion is a distinct action |

The author must understand persistence, concurrency, and recovery because the
Plugin owns them. The framework can remove lifecycle plumbing, but cannot make
an acknowledged write durable by flushing only during shutdown. Process crash
and power loss can skip that hook. Native code also needs cooperative execution
for timeouts; a stuck synchronous function is not independently preemptible.

This example needs no custom `#[resource]`, `State<T>`, named dependency, or
public preparation/activation phases. It earns one constructor and a stable
place to store data. A caller that needs existing advanced lifecycle features
can use their advanced path, with a clearly defined migration to this syntax.

## 3. Copy from one Store instance to another

Assume a product-owned Store Capability already supplies a typed `StoreClient`.
Its account or database implementation owns credentials and connection policy.
The copy Plugin owns transfer rules, retries, and its checkpoint:

```rust
#[lenso::plugin]
struct Mirror {
    #[dependency]
    source: StoreClient,
    #[dependency]
    destination: StoreClient,
    checkpoint: CheckpointStore,
}

impl Mirror {
    #[lenso::create]
    async fn create(
        source: StoreClient,
        destination: StoreClient,
        data: lenso::DataDir,
    ) -> Result<Self, CopyError> {
        Ok(Self {
            source,
            destination,
            checkpoint: CheckpointStore::open(data.path()).await?,
        })
    }
}
```

The tool method delegates to ordinary transfer code using these three fields.
No `Port`, binding collection, or runtime lookup is needed in that code.
Constructor arguments map to the annotated fields by Rust name and type, so
two clients with the same type remain distinct. The framework must connect
them by requirement identity, never select the first matching Capability.

For new declarations, propose using the field name as the stable dependency
ID. Renaming a published field must preserve its old ID explicitly:

```rust
#[dependency(id = "source")]
origin: StoreClient,
```

That removes redundant `id = "source"` from every starter, at the cost of making
the initial field name part of the public contract. Package comparison must
diagnose a removed/new dependency and offer this remedy. It must not guess that
two equal-typed fields were renamed or silently migrate their saved choices.
Human-readable labels can change without changing identity.

Installation, or the first configuration action, presents a concrete choice:

```text
Mirror / default
  Source:       Store / production
  Destination:  Store / archive

These choices will be saved. Adding another Store will not change them.
```

| Stage | What the person does | What should happen |
| --- | --- | --- |
| Create/write | Declare two clients of the existing Store interface | Generate two distinct named requirements without new Capability IDs |
| Configure | Select source and destination from permitted instances | Show meaningful labels; explain a missing provider or denied choice |
| Try | Invoke copy in a development App with disposable datasets | Read source, write destination, and report progress/errors |
| Add a third Store | Install another provider | Preserve both saved choices |
| Restart | Start the configured App | Use the same choices; fail visibly if an active dependency's target is missing |
| Change source | Select a different source explicitly | Check checkpoint/data compatibility before continuing transfer |
| Update source code | Rename a field while retaining its old ID | Preserve the chosen instance; report incompatible requirement changes |
| Disable/remove | Stop or remove the copy Plugin | Stop new transfer work, retain its choices/data as appropriate, and show retained data |

Two saved provider Instances are not sufficient to validate a checkpoint.
An account may be reconfigured in place, or a selection may point to another
dataset. The Store interface must supply suitable stable dataset identity (or
the product must require an explicit reset). Transfer code keys checkpoints by
source dataset, destination dataset, and relevant copy settings. Destination
effects need idempotency/recovery before the source checkpoint advances; a
crash between the two calls is not a cross-Plugin transaction. No generic
binding mechanism can infer these business rules.

Source and destination selecting the same provider can be valid at the
framework level. This Plugin must reject unsafe self-copy or implement an
explicit policy. These semantics belong in transfer code, not Kernel routing.

## Where do choices live, and when are they saved?

The three walkthroughs justify durable choices for actual selectable
dependencies. They do not yet justify a particular file layout or a universal
multi-file transaction system. The simple tool and local counter need no
dependency-choice files. Fixed Host attachments already have an authority and
need not produce a user-owned duplicate unless users can change them.

| Representation | Benefit | Cost |
| --- | --- | --- |
| Per-instance `.dependencies` file, as in ADR 0073 | Keeps choices next to an instance; preserves Plugin-owned config schema | More files and potential multi-file publication; older parsers must reject it |
| One App dependency-choice file | One place to inspect, export, and atomically replace all choices | Central merge contention and references to instance identity; config/package edits are still separate |
| Reserved envelope around instance config | Config and dependencies can change in one file | Breaks today's flat Plugin config format and adds a wrapper even when unnecessary |

Keep the existing flat business config for now. Compare the first two choices
using these same operations before selecting a representation. One file alone
does not make a package/config/choice change transactional; measure the actual
required operation instead of adding a generic journal by assumption.

Prefer saving selections as part of explicit install/configure. Startup should
normally consume prepared intent. A hand-edited or freshly copied root may
still need initialization: compare an explicit setup action with startup
materialization, including a read-only deployment. Interactive convenience must
not make read-only inspection write files or make a script guess an account.
This remains a discussion choice, not an instruction to implement both modes.

## Native, Process, and Wasm must be explained at the right moment

| Delivery route | What the developer is choosing | Implication for these examples |
| --- | --- | --- |
| Native Rust | Link trusted code into the App | Suitable for built-in official Plugins; typed local calls can avoid process serialization; updating code normally rebuilds the Host |
| Rust Process | Install a compatible executable run by the Host | Candidate default for independently installed Rust Plugins; OS-targeted artifact and process-call overhead; separate process is not automatically an OS permission sandbox |
| Wasm | Build for the Host's admitted Wasm interface | Portable only across compatible Hosts/profiles; imports and supported contracts determine which I/O and lifecycle behavior are available |

This table describes design consequences, not a current SDK support matrix.
Pure computation in the text tool does not prove its Tool SDK works in every
Adapter. `DataDir` cannot imply ambient filesystem access in Wasm. The counter
and copy Plugin would need an admitted storage interface with equivalent
semantics or must declare that target unsupported. Arbitrary Cargo dependencies
can introduce OS requirements; source attributes alone cannot discover all of
them or guarantee portability.

The starter should explain its default runtime immediately. An explicit Wasm
choice must show relevant limits before the developer writes substantial
stateful code. Compilation catches unsupported dependencies where possible;
packaging and Host admission check the exact remaining profile/contract needs.
No silent target fallback, no forced rewrite to TypeScript, and no claim that
every Rust source compiles unchanged into every runtime.

## Recommended simplifications and remaining discussion

Keep the struct-based Plugin as the common form. Prefer explicit typed
constructor parameters over a generated per-Plugin Inputs type. Let dependency
field names establish IDs initially, with explicit ID preservation on rename.
Make local private state ordinary Rust code with a narrow admitted directory
input, rather than requiring another Plugin or a generic state service.

Keep mandatory internal identities, compatibility checks, and bounded lifecycle
behavior. Their existence is justified; exposing their construction to every
author is not. Defer constructor spelling, choice-file layout, startup writes,
and a generalized transaction design until their user-facing behavior is
settled. Existing detailed candidates remain discussion material rather than
implementation authorization.

The next discussion should decide whether these three workflows feel natural,
especially typed construction, field-name stability on rename, and whether
install/configure should finish selection before startup. It should not start
by splitting implementation tickets or running the illustrative Rust.
