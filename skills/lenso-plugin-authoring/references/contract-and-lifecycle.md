# Shared Plugin Contract and lifecycle

One Plugin Release owns one runtime-independent Contract. The Contract owns the
Plugin ID and release version, root Slot, configuration Schema and defaults,
provided and required Capabilities, restart policy, criticality, and state
semantics.

Each executable implementation owns only its exact runtime package identity,
entrypoint, target, and Execution Class. Host policy selects one compatible
implementation before Plan resolution. Runtime does not benchmark, negotiate,
or fall back after startup or invocation failure; selecting another
implementation creates a new App Generation.

Several implementations may share one Release only when they preserve the same
configuration, Capabilities, success, Domain Errors, Runtime Failures,
cancellation, lifecycle, and state semantics. Otherwise split the behavior
into a different Plugin Release.

Package defaults are conservative implementation defaults. Host configuration
is product policy. `plugins/<plugin-id>/<instance>.toml` is the App owner's
typed patch. Secrets remain external references.

Every prepared Instance generation is fresh. Preparation validates resources
without exposing traffic; activation starts owned work; readiness precedes
routing; deactivation and cleanup release every task/resource; recreation does
not share mutable state with its predecessor. Stateful implementation switches
require explicit compatibility or migration evidence owned by the Plugin.

These rules are satisfied when Contract and implementation fields have one
owner, all implementations pass identical observable vectors, and a failed
selected implementation fails the candidate Generation without trying another
implementation.
