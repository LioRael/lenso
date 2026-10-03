# Plugin verification

Choose the narrowest proof that exercises the changed behavior. During ordinary
source development, use the affected package's format/typecheck or tests and a
real consumer/request through the selected Host/Adapter. Test changed success
and rejection paths. A local source Plugin does not require packing, signing,
freezing, a separate handoff or a full workspace run on each edit.

| Changed or claimed boundary | Additional proof |
| --- | --- |
| Configuration or dependency selection | Schema/default validation, `lenso app check`, selection/bindings/provenance from `lenso app show`, and affected request |
| Capability contract | Compatibility against the accepted version, generated freshness and affected published language projections/consumers |
| Tasks, state, lifecycle or resources | Readiness, cancellation/limits, cleanup, fresh recreation and affected resource behavior |
| Multiple published implementations | Same affected Contract vectors for each implementation, deterministic Host selection and unsupported-target rejection before readiness; no runtime fallback |
| Selection, install, removal or optional support | Disable/remove and resolve/start again; no hidden registration or Kernel branch |
| Runtime or target mechanics | Affected target conformance and real Host/Adapter smoke, including changed startup/failure/shutdown paths |
| Money, Auth or migration | Explicit final authority and trust boundary, allow/deny requests, affected concurrency, idempotency and recovery against real storage; rollback/resume when claimed |
| UI | Existing frontend checks, rendered result and affected interactions |

One implementation may create several Instances; changing one Instance must
not share configuration, bindings, resources, mutable state or permissions with
another. Prove isolation where this behavior changes. Separate targets still
need their own support validation, resource binding and lowering even when App
Composition and semantic resolution are reused.

For portable distribution, `lenso plugin pack` validates created bytes and
`lenso plugins add` validates received bytes. Do not add a separate `plugin
verify` step. Packing is conditional on delivery of an archive, not on every
source edit. A live Host keeps routing unchanged until a candidate Generation
passes readiness, except where the selected dev workflow explicitly stops and
restarts its Host.

Preserve repository final review and the exact candidate gate at delivery;
repeat for a changed candidate/base, not after every step. Record actual
commands and useful response evidence without secrets. Never change platform
permissions or interpret a safety refusal as permission to bypass it.
