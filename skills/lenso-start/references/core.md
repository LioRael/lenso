# Core task map

Use this map when the result changes reusable Lenso product behavior or the
framework mechanics that make it executable.

## Choose the owner

| User result | Primary Skill | First completion artifact |
| --- | --- | --- |
| Clarify product behavior or decide what is removable | `lenso-business-planning` | Plugin cards, Capability edges, and one tracer slice |
| Define a stable cross-Plugin role | `lenso-capability-authoring` | Descriptor, Schemas, generated projections, and compatibility proof |
| Create or change removable behavior | `lenso-plugin-authoring` | One Plugin Contract, selected implementation path, real invocation, and deletion proof |
| Add, configure, disable, enable, remove, or inspect an App difference | `lenso-app-configuration` | Minimal `plugins/` diff plus checked and explained derived App |
| Add Host scheduling, execution, process, transport, generation, or selection mechanics | `lenso-runtime-extension` | One narrow Driver, Adapter, Runner, or Host-policy change with real-host proof |

Use the deletion test before choosing: removing selected product behavior
should remove its state, policy, work, and operational complexity. If it does,
the behavior belongs to a Plugin. If every product needs a different host
translation of the same portable Interface, use a runtime seam.

## Common sequences

- **First Plugin:** Plugin authoring, then App configuration when a real Host
  must select it.
- **New collaboration role:** Capability authoring, then Plugin authoring for
  providers and consumers, then App configuration for the visible difference.
- **Existing App change:** App configuration first; escalate only missing root
  Slots or implementation-selection policy to runtime extension.
- **Framework gap:** runtime extension first when the portable contract already
  exists. Kernel work requires a demonstrated portable semantic gap and
  product-neutral conformance rather than a convenience gap in one Host.

The Core path is complete when one owner controls the next artifact, its exact
source is located, and success, honest failure, inspection, and removal or
replacement evidence are defined.
