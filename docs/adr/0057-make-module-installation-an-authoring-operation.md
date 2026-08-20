# Make Module installation an authoring operation

Installing a Module will be an explicit authoring-time change to an App project,
similar to adding a Cargo or npm dependency or copying owned source into the
project. Tooling updates the relevant package-manager inputs and declarative App
Composition, previews the change, and materializes a new Resolved App Plan. It
does not ask a running Kernel to install code or mutate its graph.

## Consequences

- Authors may install source, ordinary packages, OCI-hosted artifacts, or an
  explicitly configured remote UI entry according to the selected Execution
  Adapter. The author or authorized operator owns that trust decision.
- Lenso tooling may offer `add`, `check`, `generate`, `resolve`, and `run`
  workflows, but package managers remain responsible for package acquisition
  and lockfiles. Lenso does not become a mandatory registry or package-signing
  authority.
- An `add` workflow produces reviewable project diffs and may vendor or
  generate source. It never hides an inherited runtime overlay or install state
  inside Kernel.
- Removing the project dependency and Composition entry removes the Module on
  the next build and start. There is no separate durable Kernel catalog to
  reconcile.
- A future dynamic-installation product must introduce an explicit higher-level
  Module or distributed runtime design; it cannot silently weaken the v1 static
  Plan invariant.
