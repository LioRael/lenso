## Unreleased

### Features

Add a lightweight `linked-module` feature for authoring Module HTTP, Event,
Runtime, migration, and loader contributions without pulling the API, worker,
migration runner, bootstrap composition, or Tokio host boot graph.

Expose narrow public `lenso::host::runtime` and `lenso::host::outbox` authoring
facades for manifest-declared linked Module functions, schedules, and Event
handlers, with package-consumer verification.

Expose `HostLinkedModule::try_linked` for Modules whose context-bound storage or
deployment configuration can fail with a structured Host startup error.

## lenso@0.3.36

### Fixes

Publish a coherent Host dependency closure that removes the retired Remote
Module crate and resolves Auth Modules built against digest-bound Console UI
artifact contracts. Auth UI is part of its owning Module Release and is not an
independently versioned npm product.

## lenso@0.3.35

### Fixes

Publish the corrected public facade without the retired Console package type
re-exports so clean crates.io consumers compile against released contracts.

## lenso@0.3.34

### Features

Publish the signed System Plane enrollment, observability, and operations
contracts, then expose the Console and System Plane authoring boundaries through
the public `lenso` facade.

### Security

Require consumers to verify the Console-signed Offer and Service-signed Receipt
as one bilateral enrollment exchange before accepting registry evidence.

## lenso@0.3.32

### Features

Publish the M6 General Availability Support Manifest and the complete public
package closure required by its exact supported component combination.
