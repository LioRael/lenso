# Module Console Surfaces

Status: current after the 2026-07-30 Console System Plane decision.

A Module may declare Shell-rendered declarative surfaces or isolated web
surfaces through `ModuleManifest.console`. An isolated surface names an entry
and the exact `lenso.console-bridge.v1` protocol; it never names a package to be
executed in the Console shell.

Executable UI is delivered only as `ConsoleUiArtifact` inside the same immutable
Module Release. The release binds the artifact locator and digest, entry paths,
bridge protocol, requested permissions and provenance. A surface that selects
isolated presentation is invalid unless the release contains the corresponding
artifact entry.

The Console Service composition lock selects the exact Module Release and stores
the granted subset of requested permissions. Loading occurs in a sandboxed
cross-origin iframe. The shell grants only structured bridge operations allowed
by that exact `(ModuleId, Module Release digest, UI artifact digest)` grant.
Artifacts receive no shell imports, ambient credentials, direct managed-Service
network access, secret values or same-origin storage.

After a Module operation is durably accepted, the target-side management
adapter submits the complete selected artifact set to the Console Service's
`/api/console/v1/artifacts/reconcile` endpoint. The adapter permits HTTPS
locators and loopback HTTP only. The Console Service downloads each artifact,
verifies its SHA-256 digest, writes the immutable object to content-addressed
storage, and atomically records the composition receipt. The scoped
`console.artifacts.manage` credential authorizes only this reconciliation; it
does not grant a Module or its UI access to a managed Service.

Declarative surfaces are rendered by the shell from data in the manifest and do
not load executable Module code. Business administration remains a business
Module concern and does not enter the System Plane.

Managed Services never host Console assets. There is no `/console/*`,
`/console/extensions/*`, Console extension registry, copied-bundle ledger or
same-origin JavaScript compatibility lane.

See [Lenso Console System Plane Architecture](lenso-console-system-plane.md) for
composition, trust, permission and lifecycle invariants.
