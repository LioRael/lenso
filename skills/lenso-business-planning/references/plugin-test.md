# The Plugin test

Use the **deletion test**: if removing a selected product capability should
remove its behavior, state, policy, tasks, and operational complexity, that
concern belongs in an ordinary Plugin.

## Classify by responsibility

| Concern | Owner |
| --- | --- |
| Business behavior, Auth, State, Secrets, Story, Audit, OpenTelemetry, Workflow, target Web UI | Plugin |
| Stable role between consumers and eligible providers | Capability |
| Plugin Root package, keyed Instance, and typed configuration differences | App configuration |
| Default Instances, root Slots, private attachments, execution ceilings | Product Host Catalog |
| Serializable immutable execution input | Resolved App Plan |
| Scheduling, monotonic time, timers, host cancellation | Runtime Driver |
| Plugin generation, endpoint mechanics, isolation, wire or process translation | Execution Adapter |
| Driver and Adapter assembly, host shutdown, terminal outcome | Runner |
| Portable graph, lifecycle, invocation, admission, supervision, readiness, diagnostics | Kernel |

Technical infrastructure does not become a Plugin merely to satisfy the
slogan. A database pool remains a private persistence Adapter unless it
provides a genuine independently replaceable semantic role. Bun, native, and
browser-host execution remain Driver/Execution Adapter choices; the Web
Browser Adapter that projects generated clients into a target UI is itself a
selected Plugin. Authoring tools edit projects and materialize Plans; they do
not join the runtime graph.

## Fast examples

| Request | Classification | Deletion reasoning |
| --- | --- | --- |
| "Authenticate these credentials" | Auth Plugin | removing Auth removes credential policy and assertion work |
| "Store Orders durably" | Orders Plugin private persistence Adapter | removing Orders removes its schema and storage behavior; a global database Plugin adds no semantic owner |
| "Run this Plugin in Bun" | Bun Execution Adapter choice | deleting the business feature does not delete process/wire mechanics from the host |
| "Choose one Secrets provider" | Plugin Root difference plus Host Slot policy | the provider Plugin owns behavior; the Host owns eligible/default choice; the App owner only adds, disables, or configures an Instance |
| "Retry a crashed generation" | Kernel supervision plus Adapter recreation | the mechanism applies product-neutrally; Plugin owns only truthful recovery/state semantics |
| "Expose the Orders page" | UI Contribution Plugin plus Web app configuration | removing the page removes its route/assets/requirements without changing Kernel |

## Shape a Plugin vertically

A cohesive Plugin owns:

- the meaning and lifecycle of its facts;
- its business rules and final authorization;
- its preparation, activation, deactivation, and managed resources;
- private storage or a required semantic persistence Capability; and
- the Capabilities it provides and explicitly requires.

Split a Plugin when ownership, lifecycle, trust, failure policy, release
cadence, or a proven deployment boundary diverges. Keep it whole when the only
argument is file count, framework vocabulary, or a hypothetical future
service split.

A separately deployed process still implements an ordinary Plugin through an
Execution Adapter unless it has become a separately authoritative App with its
own product boundary. Deployment topology alone does not create a new product
type.
