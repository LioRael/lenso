# Keep the System Plane out of business traffic

The System Plane will coordinate topology, contracts, releases, policy, configuration, and aggregated operational evidence, but it will not proxy business requests or participate synchronously in Service execution. Existing Services continue operating from their last valid state when the System Plane or Runtime Console is unavailable, so control-plane failure can pause coordination without becoming a Data Plane outage.

## Consequences

- Services communicate directly over their configured request and event transports.
- Runtime identity, routing, and policy decisions cannot require a round trip to one central Host for every operation.
- System Plane availability may gate new configuration, releases, or discovery updates, but not established business traffic.
- Runtime Console aggregates evidence and control actions without becoming a runtime dependency.
