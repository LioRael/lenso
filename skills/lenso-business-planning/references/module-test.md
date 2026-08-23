# The Module test

Use the **deletion test**: if removing a selected product capability should
remove its behavior, state, policy, tasks, and operational complexity, that
concern belongs in an ordinary Module.

## Classify by responsibility

| Concern | Owner |
| --- | --- |
| Business behavior, Auth, State, Secrets, Story, Audit, OpenTelemetry, Workflow, target Web UI | Module |
| Stable role between consumers and eligible providers | Capability |
| Package inputs, keyed Instances, configuration, bindings, placement | App Composition |
| Serializable immutable execution input | Resolved App Plan |
| Scheduling, monotonic time, timers, host cancellation | Runtime Driver |
| Module generation, endpoint mechanics, isolation, wire or process translation | Execution Adapter |
| Driver and Adapter assembly, host shutdown, terminal outcome | Runner |
| Portable graph, lifecycle, invocation, admission, supervision, readiness, diagnostics | Kernel |

Technical infrastructure does not become a Module merely to satisfy the
slogan. A database pool remains a private persistence Adapter unless it
provides a genuine independently replaceable semantic role. Bun, native, and
browser-host execution remain Driver/Execution Adapter choices; the Web
Browser Adapter that projects generated clients into a target UI is itself a
selected Module. Authoring tools edit projects and materialize Plans; they do
not join the runtime graph.

## Fast examples

| Request | Classification | Deletion reasoning |
| --- | --- | --- |
| "Authenticate these credentials" | Auth Module | removing Auth removes credential policy and assertion work |
| "Store Orders durably" | Orders Module private persistence Adapter | removing Orders removes its schema and storage behavior; a global database Module adds no semantic owner |
| "Run this Module in Bun" | Bun Execution Adapter choice | deleting the business feature does not delete process/wire mechanics from the host |
| "Choose one Secrets provider" | App Composition binding | the provider Module owns resolution; Composition owns which Instance satisfies the role |
| "Retry a crashed generation" | Kernel supervision plus Adapter recreation | the mechanism applies product-neutrally; Module owns only truthful recovery/state semantics |
| "Expose the Orders page" | UI Contribution Module plus Web composition | removing the page removes its route/assets/requirements without changing Kernel |

## Shape a Module vertically

A cohesive Module owns:

- the meaning and lifecycle of its facts;
- its business rules and final authorization;
- its preparation, activation, deactivation, and managed resources;
- private storage or a required semantic persistence Capability; and
- the Capabilities it provides and explicitly requires.

Split a Module when ownership, lifecycle, trust, failure policy, release
cadence, or a proven deployment boundary diverges. Keep it whole when the only
argument is file count, framework vocabulary, or a hypothetical future
service split.

A separately deployed process still implements an ordinary Module through an
Execution Adapter unless it has become a separately authoritative App with its
own product boundary. Deployment topology alone does not create a new product
type.
