# Optional Engine Web and Contracts authoring

`lenso-engine-web` and `lenso-engine-contracts` are optional authoring processors.
Both implement the existing Engine `Plugin::plan/process` interface, consume
immutable `Snapshot` inputs, and emit normal Engine resources. Neither changes
Kernel, Runtime Drivers, HTTP execution, Plugin identity or Host selection.
The defaults are replaceable policies, not required directory layouts.

Shared acquisition is scoped to one selected-input epoch. App planning,
convention and contract baselines, source provenance and native dependency
fingerprints hash the bytes and directory membership acquired in that epoch.
Overlapping roots reuse physical inputs. Independent sessions verify inputs
before execution, at compiler/native build boundaries, and before publication;
those safety observations intentionally read again. Generated projection writes
invalidate their changed files and ancestor listings before further acquisition.

`acquisition.json` counters describe that acquisition session at staging time,
not all filesystem reads in an App build. Distribution-marker validation,
lock/authority checks, compilers and Cargo subprocesses have separate reads.
The shared fingerprint regression exercises the actual digest entrypoints used
by planning, assembly and native dependencies alongside discovery and staging;
it proves reuse in that input epoch, not a single scan across the full pipeline
or across processes. A deterministic App-process test rejects a body-only edit
after discovery and before fingerprinting even when Plugin selection is equal.

## Web

The official source App Web starter opts into the versioned preset:

```toml
[package.metadata.lenso.web]
preset = "v1"
```

Its default tree is `src/lib.rs` (one source-declared Plugin),
`src/app/**/route.rs` (filesystem routes), and optional `src/routes/**/*.rs`
(explicit routes). The App infers the root Plugin provider and stages generated
bindings into an immutable package before building its ordinary native Host.
The authored project needs no processor registration, `build.rs`, or generated
`include!`. Use App build/dev or Plugin dev for this preset; plain Cargo on the
authored package does not perform App lowering. Existing Cargo/build-script
projects keep their explicit pipeline. A custom build script cannot also opt
into the preset.

For example, `src/app/users/[id]/route.rs` contains:

```rust
#[get]
#[route_id("users.read")] // optional stable ID; default is "get:/users/{id}"
#[middleware(authenticate)]
async fn read(&self, Path(user): Path<UserPath>) -> Result<Json<User>, Problem> {
    self.lookup(user.id).await
}
```

These are supported authoring APIs; the owning Plugin supplies the types and
middleware methods. A method attribute is mandatory. Root `route.rs` is `/`,
`[id]` is `{id}`, final `[...rest]` is `{*rest}`, and `(group)` contributes no URL
segment. Optional catchalls, malformed names, traversal, repeated parameters,
overlapping filesystem roots and ambiguous route shapes fail with provenance.
An ID override preserves Plugin identity and manifest selection; the default
route ID changes when its method or derived URL changes.

Explicit handlers support `#[get("/health")]` (function name as ID), an optional
`#[route_id("health")]`, and the unchanged `#[get("health", "/health")]` form.
Both styles combine into one validated RouteSet and one existing Endpoint
implementation. Duplicate IDs and method/path conflicts across styles fail.
Different methods may share a URL. There is no second router or runtime.

`WebOptions.middleware` supplies provider middleware. `WebOptions.scopes` maps
physical directory prefixes to additive middleware lists, e.g.
`{"src/app": ["authenticate"], "src/app/admin": ["require_admin"]}`. The preset
accepts these same fields in its TOML table. Order is provider, outer directory,
inner directory, handler; each list retains declaration order. Children cannot
remove ancestor middleware. Route groups remain physical scopes. Existing
Endpoint dispatch then extracts arguments in signature order before invoking
the handler. Custom `FromRequest<Provider>` extractors, short-circuit responses,
authorization ownership, errors and cancellation keep their existing semantics.
No authentication is inferred from directories.

```rust
let options = lenso_engine_web::WebOptions {
    provider: "Gateway".into(),
    roots: vec!["endpoints".into()],
    exclude: vec!["endpoints/private".into()],
    output: "http/routes.rs".into(),
    ..Default::default()
};
lenso_engine_web::build(source_root, out_dir, options)?;
```

The default reader selects nested `.rs` handler sources under `src/routes`.
One file can contain several functions. Each handler declares a method and path
with the existing HTTP attributes; its ID may be explicit or derived. Types, state and helper
methods stay in the owning source module. The processor validates IDs, methods,
path parameters and matcher conflicts, then emits an inherent `#[endpoint]`
implementation. The official macro owns typed extractors and Endpoint bindings;
Web Ingress still validates the combined routes before readiness.

Use `WebOptions.entries` for exact files with any filename or extension; these
replace root/extension discovery. Use `compile(snapshot, options)` or register
`WebAuthoring` directly when custom discovery supplies the bytes. Exclusions
are relative path prefixes. `RouteSet::validate` is available to custom parsers;
custom compilation uses the existing Engine processor/compiler seam. Empty
selection fails by default; `allow_empty` emits no Endpoint implementation.
`register_plugin = false` selects the official standalone macro mode for an
existing grouped Plugin capability registration. It does not register another
Plugin or change instance authority.

The copyable [Engine Web example](../../examples/engine-web/README.md) uses a
custom directory and excludes an intentionally uncompilable file. Web scaffolds
call this same support rather than embedding their own source compiler.

## Contracts

An application's `lenso.contracts.json` can select a default discovery policy:

```json
{
  "roots": ["interfaces"],
  "descriptor_filename": "role.json",
  "exclude": ["interfaces/drafts"],
  "targets": [
    {"kind": "typescript", "output": "client/{name}.ts"},
    {"kind": "rust-runtime", "output": "{contract}/runtime.rs", "module": "{name}"}
  ],
  "module_overrides": {"interfaces/ai-execution": "execution"}
}
```

`{contract}` is the relative descriptor directory; `{name}` is its snake-case
basename. The descriptor's contents remain the Capability identity authority.
The defaults are `contracts/**/capability.json` and one TypeScript projection.
Targets are explicit because native Rust, Process Rust, runtime codecs, WIT and
TypeScript are different consumer requirements. Existing metadata declarations
remain authoritative in App synchronization.

```sh
lenso-engine-contracts generate . --config lenso.contracts.json
lenso-engine-contracts check . --config lenso.contracts.json
```

`discover` returns a Snapshot and selected `ContractInput` values. Replace it
entirely by constructing those values and snapshots yourself, then register
`ContractAuthoring::from_inputs` in any Engine. `run` provides a bounded output
installer and optional consumer-local cache. `Mode::Check` never writes.
`ContractInput.baseline` names a previous accepted descriptor and schema closure
in the Snapshot; the official compatibility checker rejects incompatible changes.
App synchronization supplies its existing accepted baseline automatically.
The default CLI policy stores accepted descriptor/schema snapshots in an atomic
Engine publication under `.lenso/contracts/accepted`, keyed by Capability ID.
Successful `generate` advances that publication after validation and output
installation; `check` leaves it unchanged. Set `baseline_root` to another relative
directory or `null` to replace this policy with explicit baselines. Root-level
custom descriptor inputs require an explicit baseline policy.

Only the selected descriptor and mandatory referenced schemas are read. Discovery
exclusions cannot erase a selected contract's required schemas. Symlinks, escaped
paths, invalid references, output conflicts and overwriting authored files fail.
All contracts validate before projection installation. Exact matching prebuilt
projections are reused without changing their timestamps.

## App and dependency inputs

The existing App assembly/dev pipeline uses the contract processor for descriptor
projections. App contract selection is configurable in `lenso.toml`:

```toml
[contracts]
roots = ["interfaces"]
exclude_paths = ["interfaces/drafts"]
exclude_packages = ["unused-contract-package"]
dependency_output = ".lenso/contracts/dependencies"

[[contracts.dependency_projections]]
projection = "typescript"
output = "client.ts"
```

Local Cargo/npm contract metadata supports multiple projections and an optional
Rust `module` on each projection. Source-owned Rust extraction retains its
declared, trusted generator cohort. It is selected authoring work, not scanning.
Successful App assembly records input/projection freshness evidence;
`app check` rejects changed schemas, source, metadata, locks, outputs or generator
cohort instead of regenerating. Prepared production artifacts keep their existing
locked artifact validation; production startup runs no generator.

Selected Cargo normal/build dependency closures include registry and git
contracts with published descriptor/schema inputs. External inputs require a
preexisting lock; dependency acquisition/preparation is separate from scanning.
Normal starter installation prepares that lock before discovery. `--no-install`
leaves dependency preparation to the caller; discovery diagnoses a missing lock
without resolving dependencies or creating one. A non-resolving Cargo probe
identifies the actual workspace owner, so an unrelated ancestor lock is never
borrowed. Dependency-free local descriptor packages need no lock.
Published descriptor snapshots are consumed even if package metadata also names
Rust authoring source. Scanning/extraction never executes dependency build scripts
or source exporters. A source-only external package must publish its snapshot.

External outputs live under the configured consumer dependency directory, keyed
by exact package identity. Missing targets use the official generator. Cache
keys include Snapshot bytes, lock inputs when provided by App dependency discovery,
projection/module/output policy, support version, generator version and generator
source fingerprint. An already materialized locked dependency and cache work
offline; missing input materialization remains an actionable prerequisite.
Source directories in Cargo registry/git caches are never output destinations.
Published prebuilt projections are reused when their declared target/module,
generated marker and resolved contract digest match. Their source bytes join
the input-change and freshness checks; older compatible generator cohorts can
be copied into consumer outputs without regeneration. Missing or mismatched
targets retain the official generation and runtime-cohort safeguards.
Custom input providers must supply their own locked provenance and tool inputs.

A custom Host can use these processors directly and run its own `Mode::Check`
before packaging. Engine App inspection separately requires a persisted Host
Catalog, Root admission and App freshness record. Discovering source Plugins or
generating projections does not create that authority. A custom Host without it
can pass runtime tests while `project_facts` reports unresolved composition.

## Convention extension seam

Existing `conventions { entries, compiler }` keeps its default filename selection.
A surface can explicitly name `convention`, pass a bounded `options` JSON value,
and scope `owner` to one declaration in a multi-Plugin package:

```json
{"entry":"custom.input","owner":"example.gateway","convention":"example.web","required":true,"options":{"provider":"Gateway"}}
```

An explicit convention must be adopted through existing Plugin Root policy.
Compiler options have no generic filename/language semantics; support interprets
them. In multi-Plugin packages, support declarations and surfaces require an
explicit `owner`, avoiding duplicated package-level attachment. Compilation and
final artifact admission retain their existing bounds and validation.
