# Require Service Reliability Contracts

Every Autonomous Service Release will include a Reliability Contract covering availability and latency targets, dependency criticality, startup/readiness/liveness meaning, Degraded Modes, queue and workflow backlog limits, error budget, failure-domain expectations, canary success, and rollback triggers. Reusable Reliability Profiles keep the requirement practical for small teams, while Policy Packs and Runtime Console evaluate the same declared expectations.

## Consequences

- Call Policies remain operation-level safety contracts; Reliability Contracts describe whole-Service behavior.
- Dependencies are explicitly critical, degradable, or optional instead of being inferred from topology.
- Health endpoints must represent declared semantics rather than return generic process status.
- Releases can pause or roll back from objective Service evidence.
- Development, standard, and critical profiles may provide increasingly strict defaults without forcing every team to design an SRE program from scratch.
