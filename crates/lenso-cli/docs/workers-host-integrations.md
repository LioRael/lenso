# Plugin-owned local Workers integrations

The source candidate supports a static linked Rust App through the ordinary
Plugin Root resolver, Native Adapter and Kernel. Select this profile with
`--wasm-bindgen`; select the existing restricted Component profile with `--jco`.
These are separate build choices with separate admission checks.

## Build a static Rust App

```sh
lenso app build --target workers --root ./app --out ./dist-workers \
  --workers-runtime /path/to/pinned/workers-runtime \
  --wasm-bindgen /path/to/wasm-bindgen \
  --workers-facilities ./profiles/workers.json \
  --workers-host-limits ./profiles/workers-limits.json
lenso app explain --root ./dist-workers --json
```

The profile is `lenso.linked-rust-workers@2`. It requires wasm-bindgen 0.2.127,
`wasm32-unknown-unknown`, linked Cargo Plugins, one main execution lane, Request and Stream
Capabilities, and the selected Web Ingress. It admits nonempty configuration,
multiple Instances and exact named Port bindings from the same resolved App
Plan used by the Native build. Requests invoke typed providers through the
Kernel. Unsupported Event Capabilities, WebSocket upgrades, convention compilers, published-resource
loading, dynamic loading and restart supervision fail before publication.

The linked profile emits compatibility date `2026-09-26`, paired locally with
Wrangler 4.143.1 and workerd 1.20260926.1. The restricted Component profile keeps
its existing configuration and pinned runtime cohort.

The build first uses the ordinary source Host assembly to validate source trust,
Plugin discovery, configuration and bindings. Adopted code retains the existing
`--trust-linked-build PLUGIN_ID@VERSION=sha256:DIGEST` authorization. Linked Rust
and owner JavaScript execute as trusted Host code; facility matching does not
sandbox that code or grant database permissions.

Each HTTP event constructs its own Kernel App and typed Plugin Instances.
Configuration and bindings are static; Plugin memory is recreated for each
event. Persist state through an explicitly selected owner backend. An in-memory
backend does not supply cross-request CAS or restart persistence.

Source Host owners can restrict an existing Many requirement with repeated
`--host-many-slot CONSUMER=CAPABILITY=PROVIDER_SLOT` arguments on `app build`
or `app assemble`. For example, `example.console=example.auth@1=console-auth`
selects compatible providers from that Host Slot, excluding another auth role
in a different Slot. The static linked Workers build carries the same policy
through ordinary source assembly. It uses `LocalManySlotBinding`; it does not
write a Plan or grant selectable Many choices to the App Plugin Root. Duplicate
policies, unavailable Slots and non-Many requirements fail before publication.

The generated Workers Host uses the same Stream Endpoint contract and Web
Ingress routing as Native. Response headers and each chunk leave incrementally,
with one receive per transport pull. The generated JavaScript body reader returns
clean EOF only after the provider's successful terminal and clean shutdown of
that request's independent App.
Cancellation, deadlines and failed terminals release the same request-owned
resources; failed reads remain errors, and clean cleanup does not turn a failed
terminal into success. The shared Wasm
generation stays leased until session cleanup completes. Applications supply
one Plugin source tree for both targets, without maintaining a Workers Host or
copying framework source. Provider-owned D1/PG differences remain in that
provider's infrastructure adapter, not in generated Host code.

External HTTP completion is a separate platform boundary. The selected local
workerd 1.20260926.1 direct HTTP socket sends a complete chunked response and
normally ends the connection after both a native `ReadableStream` error and a
generated App terminal error. Its KJ output sink does not implement aborting
the HTTP body writer, whose [destructor emits the final chunk](https://github.com/capnproto/capnproto/blob/fda9aecf8120d92f2085c9625330ffd9ddca7c24/c%2B%2B/src/kj/compat/http.c%2B%2B#L2636-L2648).
The Host therefore
does not guarantee an abnormal client EOF after headers. Internal stream failure
remains observable, but network EOF does not prove a successful App or business
terminal. Applications retain their established explicit terminal/proof semantics.
Generator/session correctness and external transport qualification must be
recorded separately; this local result does not qualify production Cloudflare.
The fixture's strict external failure-EOF gate remains a separate failing check.

## Owner facilities

A Plugin owns each concrete adapter and its configuration validation. Declare
its private typed input in the Root and its factories in Cargo metadata:

```rust
#[lenso::plugin]
struct StateRoot {
    #[facility(id = "state")]
    state: StateHandle,
}
```

```toml
[package.metadata.lenso.host-facilities.state]
native = "host_facilities::state"
workers = "host_facilities::state"
workers-adapter = "src/host_facilities/state.mjs"

[package.metadata.lenso.host-facilities.cache]
native = "host_facilities::cache"
workers = "host_facilities::cache"
workers-adapter = "src/host_facilities/cache.mjs"
```

The Native factory has signature
`fn(&serde_json::Value) -> Result<StateHandle, RuntimeFailure>`. It receives one
operator-supplied slot value and runs when its generation constructs the typed
input. The Workers Rust factory has signature
`fn(&wasm_bindgen::JsValue) -> Result<StateHandle, RuntimeFailure>`. Its private
JavaScript module exports synchronous `create(binding, scope, configuration)`;
the returned adapter performs asynchronous I/O through the event scope. The
builder stages that owner module and records its digest. Use a self-contained
module; this profile does not package an npm dependency tree for owner adapters.

The grant file uses exact resolved Instance keys and named slots. For Workers:

```json
{
  "schema": "lenso.host-facilities.v1",
  "instances": {
    "example.state/primary": {
      "state": {"binding": "PRIMARY_DB", "configuration": {"schema": "private_a"}},
      "cache": {"binding": null, "configuration": {"enabled": false}}
    },
    "example.state/secondary": {
      "state": {"binding": "SECONDARY_DB", "configuration": {"schema": "private_b"}}
    }
  }
}
```

The Host passes only each selected binding to its owner factory. A binding must
be an own property of the event environment. Explicit `null` passes no binding;
its meaning and whether that profile is supported belong to the owner. Missing
bindings, unknown Instances and undeclared slots fail before business admission.
Two Instances may select different authorized schemas on one physical PG, or
independent D1/PG backends, without relying on array order. Business configuration
and Capability requirements remain separate from these Host grants.

For Native, slot values use the owner's Native schema rather than the Workers
`binding`/`configuration` shape. Build the ordinary Native distribution, then run
`lenso app start --root ./dist --host-facilities ./profiles/native.json`.
Credentials remain in operator-managed sources consumed by the owner factory.
Startup validates readiness; it never runs operator migrations automatically.

A Native owner that compares invocation deadlines can opt into the exact Host
Driver clock with `native-clock = true` in that slot's Cargo metadata. Its
factory signature becomes
`fn(&serde_json::Value, &lenso_native_adapter::NativeHostClock) -> Result<Handle, RuntimeFailure>`.
The owner may retain `clock.clone()` and call `clock.now()`. The generated Host
passes the same Tokio Driver to that clock and the Kernel; it does not create a
second `Instant` epoch. Factories still construct a fresh typed input per
generation. Slots without this opt-in retain the single-argument factory.

## Budgets, admission and evidence

An optional `--workers-host-limits` file contains a flat JSON object of explicit
Host limits. For example:

```json
{"eventLimitMs":10000,"cancellationLimitMs":1000,"cleanupTimeoutMs":1000,"maxOperations":128}
```

Known fields are event/session/cancellation durations, retirement admission
count, maximum concurrent events, HTTP body/head/read bounds, and event cleanup
duration/operation count. Values must be bounded positive integers. Scope limits
go to the generated scope factory; the rest go to the Host facade. Omission
preserves the runtime defaults, including the 1000 ms event deadline and 250 ms
cleanup budget. Size budgets using observed infrastructure latency. A deadline
does not establish rollback; an unconfirmed write requires owner reconciliation
and is never automatically replayed.

Response transport defaults to a 64 KiB chunk bound and a 64 MiB cumulative
body bound. Configure `maxResponseChunkBytes` up to 65536 and
`maxResponseBodyBytes` up to 67108864. Buffered Endpoint responses retain the
1 MiB Wasm bound and are sliced into transport chunks. Session duration and
cancellation cleanup have separate runner budgets; slow or abandoned readers
cannot retain an event indefinitely.

Contract source can set `request_queue_capacity` and `request_max_concurrency`
together on `#[capability(...)]`. A Root can set those same fields on
`#[plugin(...)]` to override the default admission of its own provided
Capabilities, including an HTTP Endpoint, without changing the shared contract.
Concurrency must be positive. Existing source without these fields retains its
default policy. Owners must ensure their implementation supports the selected
concurrency; a bounded queue does not make private mutable state concurrent.

`workers-build.json` records the exact graph, Wasm/bindings/entrypoint bytes,
runtime modules, owner modules, grants and explicit budgets. `app explain`
checks those persisted digests and reports Instances, named requirements,
selected providers, authorized binding names and rejected/missing grants. It
does not invoke factories, read credentials, write configuration or probe a DB.
It reports live readiness as `not_run`/`not_observed`.

Build, local workerd behavior and deployed Environment-plus-Infrastructure
qualification are distinct evidence. D1 and PostgreSQL acceptance belong to the
owning fixture. Remote Hyperdrive qualification is deferred for this candidate;
it must remain `not_run`. Source candidates do not establish registry publication.

## Restricted Component integration

An import-free HTTP Component may need a Plugin-owned JavaScript Host entrypoint
for private infrastructure. The local Workers builder can assemble that code
without knowing the Plugin's identity or business protocol. The following source
integration applies to the restricted Component profile.

The Plugin owner supplies a profile and its Host files. The Host operator
separately approves the exact profile digest:

```sh
lenso app build --target workers --root ./app --out ./dist-workers \
  --workers-runtime /path/to/pinned/workers-runtime \
  --jco /path/to/jco \
  --workers-integration /path/to/integration/integration.json \
  --trust-workers-integration sha256:APPROVED_PROFILE_DIGEST
```

Both integration arguments are required together. Review the Host code and
obtain the digest from that reviewed source; a digest supplied by an untrusted
download is not independent authorization. These modules execute as trusted
Host JavaScript, outside the Component's isolation boundary. Bundle verification
does not authorize them, and this mechanism does not add a sandbox.

## Source contract

The profile uses `lenso.workers-integration.v1` with these required fields:

| Field | Meaning |
| --- | --- |
| `plugin_id` | Exact selected Plugin ID |
| `instance_key` | Local suffix, such as `default`; the builder compares the full resolved Plan key |
| `authoring_version` | Exact selected source authoring version, 1 or 2 |
| `world` | Owner-declared private WIT world label, not an independent compiler attestation |
| `manifest_digest` | Canonical verified Bundle manifest SHA-256 |
| `artifact_digest` | Selected Component SHA-256 |
| `runtime_version` | Pinned generic runtime version, `0.1.4` or `0.1.5` |
| `files` | Map of each staged filename to its SHA-256 |

Every digest has the form `sha256:` followed by 64 lowercase hexadecimal digits.
Unknown profile fields are rejected. The builder checks the operator's pin
before interpreting the profile, then matches its identities against the
ordinary resolver and Bundle selector; the profile cannot select another
implementation or edit the Plan.

Keep the profile in a dedicated directory containing only that file and the
declared assets. Include `worker.mjs` and `README.md`; other assets must be flat
ASCII `.mjs` filenames. Paths, subdirectories, symlinks, undeclared files and
framework output names are rejected. There may be 2–16 assets, each at most
1 MiB and at most 16 MiB in total; the profile is limited to 64 KiB. Changed
inputs fail the final source recheck before output publication.

The entrypoint may import the generated `plan.mjs`, `descriptor-digests.mjs`,
`artifact.mjs` (`world` and `digest`), and Jco's `guest.js` / `guest.core.wasm`.
Generic runtime modules are independently pinned by the builder. Runtime
`0.1.5` supplies `component-admission.mjs` as well as `component-requests.mjs`;
the runtime package no longer supplies product-specific bridge code.

The integration owns its private export checks, request mapping, resource
authorization, deadlines, error semantics and operational README. The builder
does not interpret its business protocol. Keep those checks in the Plugin's
integration tests and exercise the actual local runtime before claiming support.

## Build evidence and limits

The distribution contains the exact approved profile as
`workers-integration.json`. The `integration` object in `workers-build.json`
records that filename, its digest, selected identities and asset digests.
The profile and output bytes can be checked against that receipt. This is
source/digest verification, not publisher-signed Bundle ownership of sidecars.

The ordinary target restrictions remain: one selected HTTP Endpoint Instance,
no Capability bindings or required Capabilities, empty configuration, no
selected convention compiler, and an import-free single-core Component.
Jco must still match the pinned version and output closure. Omitting the
integration selects the ordinary HTTP entrypoint for every Plugin identity;
there is no business-specific fallback.

A build receipt proves neither deployment nor infrastructure availability.
Refer to the exact Environment-plus-Infrastructure test result, not merely
the target name.

Known stream failures still reject response reads. A clean shutdown receipt marks
them as a failed terminal without retiring unrelated request Apps. Unconfirmed
Rust App shutdown rejects the session receipt and retires the Wasm generation.
A native resource-scope cleanup failure rejects and fences that request; it does
not itself retire unrelated request Apps. The generated Host retains the full
runner cleanup Promise in the Workers execution context.
