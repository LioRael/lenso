# Make safe Module extraction the hero workflow

Lenso's primary microservice workflow will move a boundary-hardened linked Module into an Autonomous Service through a reviewable Extraction Plan rather than treating greenfield Service scaffolding as the main product story. The plan detects boundary and data ownership violations, freezes contracts, prepares Workloads and Service Clients, updates the System graph, defines compatibility and behavior evidence, and stages reversible Cutover without automatically executing irreversible data movement.

## Consequences

- `service create` remains available for genuinely new boundaries but is not the differentiating workflow.
- Extraction readiness becomes measurable before a team takes on distributed-system cost.
- Data backfill, coexistence, traffic switching, verification, and rollback are explicit phases.
- Runtime Stories and contract evidence compare linked and Service behavior around Cutover.
- Architecture checks must make linked Modules increasingly extraction-ready before extraction is requested.
