# Complete single-region autonomy before multi-region

The first Autonomous Service milestone will support independently released, multi-instance Services within one Operating Region, not multi-region active-active execution. Service References, Event Envelopes, Story Context, identity, and release evidence will retain Operating Region and Failure Domain metadata where relevant so active-passive recovery and later scenario-specific multi-region work remain possible without committing now to global conflict resolution.

## Consequences

- The first roadmap does not promise cross-region data conflict resolution, workflow leadership transfer, global event ordering, or automated global traffic failover.
- Disaster recovery and active-passive operation may be added before active-active behavior.
- Scale, resilience, and independent delivery must be proven inside one Operating Region first.
- Multi-region work requires a concrete business scenario rather than becoming a generic framework checkbox.
