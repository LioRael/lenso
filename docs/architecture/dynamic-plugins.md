# Plugin Generation control plane

Status: current internal runtime contract above Kernel.

The public model contains only Host, Plugin Root, Plugin, and derived App. This
document names the machinery a Host uses after resolving that public intent; it
does not add authoring concepts.

## Boundary

Kernel receives one complete immutable `ResolvedAppPlan`. It does not discover
Plugins, read `plugins/`, verify bundles, select Artifacts, persist rollout
state, or decide whether a new App may replace the current one.

The Host-owned control plane performs those jobs around Kernel:

- resolve Host Catalog plus Plugin Root into one exact Plan;
- verify every non-embedded Plugin Bundle before admitting its bytes;
- open exact Artifacts and bind them to the resolved Plugin Instances;
- construct one immutable Generation from the Plan, Artifacts, Host build, and
  resolution authority;
- stage the whole Generation behind one Ready Gate;
- atomically route new work to it while existing work keeps its old lease;
- drain, roll back, recover, and collect Generations under bounded policy.

`Generation`, `Controller`, and `Supervisor` are necessary implementation
concepts because they represent different correctness boundaries. They are not
Plugin-author concepts and do not appear in `plugins/`.

## Authority flow

```text
Host Catalog + Plugin Root
          |
       resolver
          |
Resolved App Plan + exact Plugin Artifacts
          |
Generation Spec + resolution authority digest
          |
stage -> Ready Gate -> atomic route -> drain/rollback/collect
```

The Generation Spec is immutable evidence for one runnable candidate. The
resolution authority digest closes the exact inputs selected by the resolver;
it is not a second user-maintained lock or enabled list.

## Internal responsibilities

### Generation

A Generation owns one exact Plan and its executable inputs. Work admitted to a
Generation never migrates to another one. Structural changes create a new
Generation; a compatible in-place Plan Transition may remain an optimization,
but cannot weaken this authority boundary.

### Controller

The Controller serializes transition, routing, maintenance, and shutdown
commands. It is the command boundary that prevents two callers from applying
competing transitions concurrently.

### Supervisor

The Supervisor persists and reconciles live Generation records, readiness,
routing epochs, leases, drain state, standby state, rollback eligibility, and
terminal failures. Recovery fences stale authority before publishing a route.

### Runtime storage

Durable control records and immutable Generation evidence live below
`.lenso/runtime/` or an equivalent Host-private backend. They are derived
runtime state, not App configuration. The App owner's only composition state is
the Plugin Root.

## Retired concepts

There is no public or canonical Plugin Store, install Receipt, hand-authored
Manifest, Plugin Set Lock, Desired State document, or App Definition. Bundle
verification happens at the receiving boundary, and immutable bytes may be
cached internally without turning that cache into user intent.

There is also no separate application behavior unit below Plugin. A Plugin
Descriptor directly declares configuration, provided and required
Capabilities, execution class, lifecycle policy, and state contract for one
Plugin Release. A Plugin Instance is the unit placed in the Plan and managed by
an Execution Adapter.

## Invariants

- One routed request is pinned to one exact Generation.
- A candidate becomes routable only after every required lane and Plugin
  Instance is ready.
- Failed staging leaves the current route unchanged.
- Rollback selects only a recorded ready predecessor under the exact policy.
- Recovery never guesses from directories, process state, or partial records.
- Runtime records cannot activate a Plugin absent from the resolved App.
- Kernel remains independent of product discovery, persistence, and rollout
  policy.

See [Plugin Root and App resolution](plugin-root-resolution.md) for the public
input and [Plugin execution classes](plugin-execution-classes.md) for Artifact
execution.
