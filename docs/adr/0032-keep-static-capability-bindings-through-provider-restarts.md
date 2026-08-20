# Keep static Capability bindings through provider restarts

The initial Kernel will resolve one static Module graph during App boot and keep each resulting Capability binding stable until shutdown. If a Bun child-process provider fails, its handles report that the provider is unavailable while supervision applies the configured restart policy; the Kernel neither recomputes the graph nor unloads and reactivates every consumer.

## Consequences

- Module lifecycle is `Loaded`, `Prepared`, `Active`, `Draining`, and `Stopped`, with failure recorded separately.
- The Kernel reverses runtime registrations, listeners, timers, tasks, and connections that were created through its managed Module scopes during shutdown or failed preparation. Work created through raw platform APIs remains trusted Module code outside that guarantee.
- Durable external effects remain the owning Module's responsibility and are never presented as automatically reversible.
- Dynamic installation, rebinding, and dependency-graph mutation are deferred with the distributed runtime direction.
- A failed provider remains unavailable while supervision applies a bounded restart policy. Exhausting the budget for a provider on a required path or an explicitly critical Module Instance fails and exits the App; non-critical providers may remain unavailable.
