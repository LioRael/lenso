# Linked native Rust Plugin

Use this path when the product Host links the Plugin implementation directly.
Read the exact `lenso` facade and generated Capability projection selected by
the owner repository.

- `#[lenso::plugin]` defines Plugin identity and generated descriptor/factory.
- Declare a named-field struct, using `struct HealthHttp {}` when it has no state;
  the Plugin macro rejects unit structs.
- `#[lenso::provides(...)]` lowers typed Capability implementations.
- `PluginConfig` derives strict typed configuration.
- Authoring-2 fields use `#[dependency(id = "owner")] owner: OwnerClient`;
  `Option<OwnerClient>` declares optional support with a persisted explicit-none choice.
- Legacy `Port<Client>` fields declare fixed requirements. Keep named selections
  in authoring 2 instead of combining them with `#[plugin(lifecycle)]` or `#[tasks]`.
- `NativePluginRegistry::with_linked_factories()` exposes linked availability.

Generated Provider/Client types remain the collaboration Interface. Keep
another Plugin's private types and storage outside this package. Use lifecycle
only for resources or managed work owned by this Plugin Instance.

Use `#[plugin_impl]` with `#[create]` for a complete-object constructor. A
`#[lifecycle] lifecycle: lenso_native_adapter::LifecycleContext` parameter exposes
the constructing generation's cancellation token, readiness context, and managed
task scope. Register background work before returning the object, then wait for
readiness inside that work:

```rust,ignore
let ready = lifecycle.readiness().map_err(|_| "readiness unavailable")?;
lifecycle
    .spawn_local(async move {
        ready.wait().await;
        owner.run().await;
    })
    .map_err(|_| "managed task unavailable")?;
```

Waiting for readiness or calling an outbound Port inside the constructor can
prevent activation from completing. `#[stop]` receives cancellation and the
remaining Host cleanup budget; its lifecycle context rejects new tasks. Retained
construction contexts also reject tasks after their generation scope closes.

The Host Catalog owns default Instances, root Slots, private attachments, and
implementation policy. Generated registration makes the Plugin available; it
does not activate an App-owned Instance or choose a provider.

A source-owned facility may opt into the selected Driver's deadline clock with
`native-clock = true` or `workers-clock = true` under its
`package.metadata.lenso.host-facilities.<slot>` metadata. The declared Rust factory
then receives a second `&lenso_native_adapter::NativeHostClock` argument. Its
`now()` uses the same Driver as the Kernel; the type also accepts the Workers
Driver in a statically linked Rust Worker. The default factory still receives
only its binding. This monotonic clock is not an issuer's wall-clock authority.

`#[facility(id = "state")] state: Option<OwnerHandle>` accepts absence of that
private attachment in either authoring profile. A present attachment must still
have the exact type, and owner factory failures remain errors. A direct
`OwnerHandle` field requires the attachment. An owner selecting a D1 profile must
reject `None` during preparation; the optional field permits a separate Native
profile without silently selecting another resource. This private attachment
absence does not replace a Capability dependency's persisted explicit-none choice.

Workers facility metadata may select a private adapter file from one explicitly
named runtime Cargo dependency with `workers-adapter-package = "owner-package"`
and `workers-adapter = "src/workers/adapter.mjs"`. The owner must be uniquely
reachable from the selected Plugin's resolved normal dependencies. The relative
file must remain inside that owner's source directory, including after symlink
resolution. The Host copies one module of at most 1 MiB and records its owner
Cargo identity and content digest. Supply a closed module; this path does not
bundle imports or discover npm dependencies.

This path is complete when the linked factory is discoverable in the exact Host
build, typed configuration fails closed, generated Capability calls exercise a
real consumer/provider path, lifecycle cleanup is observable, and removing the
Plugin leaves no hidden registration or Kernel branch.
