# Linked native Rust Plugin

Use this path when the product Host links the Plugin implementation directly.
Read the exact `lenso` facade and generated Capability projection selected by
the owner repository.

- `#[lenso::plugin]` defines Plugin identity and generated descriptor/factory.
- `#[lenso::provides(...)]` lowers typed Capability implementations.
- `PluginConfig` derives strict typed configuration.
- `Port<Client>` or `ManyPort<Client>` fields declare requirements and
  cardinality.
- `NativePluginRegistry::with_linked_factories()` exposes linked availability.

Generated Provider/Client types remain the collaboration Interface. Keep
another Plugin's private types and storage outside this package. Use lifecycle
only for resources or managed work owned by this Plugin Instance.

The Host Catalog owns default Instances, root Slots, private attachments, and
implementation policy. Generated registration makes the Plugin available; it
does not activate an App-owned Instance or choose a provider.

This path is complete when the linked factory is discoverable in the exact Host
build, typed configuration fails closed, generated Capability calls exercise a
real consumer/provider path, lifecycle cleanup is observable, and removing the
Plugin leaves no hidden registration or Kernel branch.
