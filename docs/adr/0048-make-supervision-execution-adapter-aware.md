# Make supervision Execution-Adapter-aware

Kernel supervision will preserve common lifecycle and availability semantics
without claiming that every Execution Adapter has the same physical fault
boundary. Each Adapter reports the isolation and recreation behavior it can
actually provide. An in-process native Module cannot be guaranteed to survive a
process abort or memory fault, while a Bun child-process Module can normally be
marked unavailable and recreated after its process exits.

## Consequences

- The Resolved App Plan selects a finite policy such as `never` or
  `on-failure`, together with maximum attempts, a time window, backoff, jitter,
  and a stability period. There is no implicit infinite restart loop.
- A consumer retains one stable Capability handle across supported provider
  restarts. Calls report `Unavailable` while the provider is down; successful
  recreation advances its generation and makes the same binding usable again.
- Kernel never replays an in-flight invocation after a provider failure and
  never substitutes another matching provider. Retry and failover remain
  explicit Capability or App concerns.
- Runtime Diagnostics may expose lifecycle state, availability, provider
  generation, restart decisions, queue saturation, and Runtime Failures. Kernel
  defines no universal business, database, or dependency-health meaning.
- A Module may provide an ordinary health or readiness Capability when its
  domain needs deeper semantics. That Capability cannot override authoritative
  Kernel lifecycle facts.
- Best-effort panic catching or graceful child-process shutdown may improve one
  Adapter, but it does not strengthen the documented isolation class beyond what
  the Adapter can guarantee.
- WebAssembly panic and trap behavior is treated as an Adapter or enclosing
  Runner fault boundary. Kernel never relies on unwinding to recover an
  in-process Module generation.
