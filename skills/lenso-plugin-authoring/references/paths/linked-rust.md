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

This path is complete when the linked factory is discoverable in the exact Host
build, typed configuration fails closed, generated Capability calls exercise a
real consumer/provider path, lifecycle cleanup is observable, and removing the
Plugin leaves no hidden registration or Kernel branch.
