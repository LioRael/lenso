# Contract shape

## Deep role Interface

Describe the role a consumer needs, not the provider's storage model, package
version, transport, process, or concrete type. A cohesive Plugin may provide
several Capabilities. Capability Descriptor versions evolve independently from
Plugin package versions.

Use one Capability for a cohesive role containing related Operations. Avoid
both one-Operation-per-Capability fragmentation and a universal Interface that
exposes an entire Plugin implementation.

## Interaction kinds

- **Request** has one terminal success, domain error, or runtime failure.
- **Stream** is bidirectional, bounded, cancellable, independently
  half-closable, and ends in one explicit terminal outcome.
- **Event** attempts independent bounded admission for every bound subscriber
  and reports partial outcomes. It does not imply persistence, replay,
  redelivery, ordering across subscribers, or exactly-once delivery.

Command and query are domain meanings within request Operations, not separate
Kernel interaction kinds. Each request and stream defines success and an open
set of stable Domain Error codes. Unavailable providers, deadlines,
cancellation, resource exhaustion, protocol violations, and internal failures
remain Runtime Failures outside that union.

Use a semantic State, Secrets, Auth, Story, Audit, or similar Capability when
another Plugin truly needs that role. Keep private helpers, tables, database
pools, HTTP routes, and process protocols out of the public contract.

## Portable source

A portable Capability uses a runtime-neutral Descriptor plus package-local
JSON Schema 2020-12 files. Preserve the portable value profile:

- wide integers, bytes, timestamps, and durations use their declared string
  encodings;
- missing and explicit null remain distinct;
- unknown domain-error codes and payloads remain representable; and
- shapes that discard wire data fail generation.

Generated Rust, TypeScript, or browser artifacts are projections of that one
source, never parallel handwritten contracts.

## Descriptor decisions

- `id` is the stable `namespace.name@major` series.
- `version` is the exact Descriptor SemVer, independent from Plugin package
  versions.
- `portable` states whether the contract crosses Execution Adapters.
- `cross_lane_transfer` is enabled only when the contract's values and
  semantics support Plan-declared lane transfer.
- every Operation has a stable `name`, one `interaction`, and package-local
  request/response/Domain Error Schema paths.

The Descriptor's Operation-array order has no protocol meaning. Never reuse an
Operation name for a new meaning. Cardinality, provider selection, admission
limits, and Event mailbox capacity belong to App configuration/Resolved Plan
rather than the Descriptor.
