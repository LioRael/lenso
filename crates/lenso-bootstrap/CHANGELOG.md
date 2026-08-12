## Unreleased

### Security

Fail closed when a loaded runtime or Event binding is undeclared, unbound,
duplicated, or drifts from its Module manifest identity.

### Breaking changes

Replace the unvalidated `function_registry`, `event_handlers`, and
`event_handlers_with_runtime_actions` helpers with fallible `try_*` admission
APIs so callers cannot bypass manifest-to-binding validation.

## lenso-bootstrap@0.1.25

### Features

Publish the M6 General Availability Support Manifest and the complete public
package closure required by its exact supported component combination.
