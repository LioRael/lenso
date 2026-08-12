## Unreleased

### Features

Expose a narrow public `lenso::host::runtime` authoring facade for
manifest-declared linked Module functions, with package-consumer verification.

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
