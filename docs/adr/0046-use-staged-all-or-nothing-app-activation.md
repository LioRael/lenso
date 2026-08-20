# Use staged all-or-nothing App activation

Kernel boot will proceed through `resolve/validate`, `prepare`, `activate`, and
`ready`. All declared Module Instances must prepare successfully before Kernel
activates providers in dependency order. A startup failure prevents readiness
and causes prepared or activated work to be released in reverse order.

## Consequences

- `prepare` may validate opaque configuration, connect to required resources,
  verify an owned schema, and reserve reversible resources. It does not expose
  Capabilities, start background processing, accept ingress, or apply an
  irreversible migration by default. It cannot invoke ordinary dependency
  Capabilities; initialization that needs them runs during activation after the
  providers are active.
- `activate` proceeds in dependency order. An active provider is callable by a
  downstream Module that is still activating, while externally triggered work
  and background consumption remain behind the App Ready Gate.
- Kernel opens one App Ready Gate only after every declared Module Instance has
  activated. Ingress, Worker, Scheduler, game-protocol, and similar Modules are
  ordinary Modules that prepare their resources but do not accept new external
  work until that signal.
- An `optional` Capability requirement means its binding may be absent; it does
  not make a declared but broken provider silently acceptable during initial
  boot.
- After readiness, bounded supervision may leave a non-critical provider
  unavailable when no required path depends on it. Exhausting the restart
  budget of an explicitly critical Instance or a provider on a required path
  fails and exits the App.
- Graceful stop rejects new ingress, drains or cancels in-flight work, and
  deactivates in reverse dependency order under an App-wide deadline. Cleanup
  failure or timeout is reported through Runtime Diagnostics but cannot keep
  the process alive indefinitely.
- Cleanup reverses runtime-owned resources and registrations only. Durable
  external effects remain the owning Module's responsibility.
