# M6 Security Threat Model

This versioned model is the human-readable companion to
`lenso.security-review-evidence.v1`. The executable evidence must bind every
review and finding to the exact GA Support Manifest and immutable release
subjects.

| Surface | Primary threat | Required control |
| --- | --- | --- |
| Workload Identity | forged or stale Service Principal | audience-bound SPIFFE identity and fail-closed rotation |
| Transport binding | identity accepted on the wrong transport or endpoint | bind identity, logical Service, Contract, and endpoint observation |
| Delegation | delegated actor escalation | explicit actor, tenant, authority, and expiry |
| Tenancy | cross-tenant read or write | tenant context at every Contract and Store boundary |
| Event replay and poisoning | repeated effects or blocked healthy delivery | stable envelope identity, Inbox deduplication, bounded retry, dead letter |
| Extraction and Cutover | two authoritative owners | stale-safe plans, reconciliation, fencing, explicit authority approval |
| Workflow controls | unauthorized retry, cancel, or compensation | protected plans, immutable history, declared compensation |
| Release signing | substituted source or artifact | exact commit, digest, provenance, SBOM, preflight, receipt, attestation |
| Secrets | plaintext exposure or stale credential | opaque references, provider leases, redaction, rotation |
| Backup and restore | incomplete state or replay duplication | encrypted immutable backup, state digests, exact replay checkpoint |
| Admin actions | observation surface gains mutation authority | manifest-declared action, policy, approval, audit evidence |
| Embedded Console | bearer token or capability leakage | sandboxed surface and narrow versioned bridge |
| Policy bypass | outage weakens authorization | Service-owned cached decision with fail-closed boundaries |
| Stale evidence | old observation authorizes new state | subject digest, source revision, freshness horizon |
| Agent boundaries | automation crosses production authority | named Approval Boundaries and zero-effect preparation |

Critical and high findings block GA while open. Accepted risk is valid only when
a named approval, reason, expiry, and exact finding digest are present.
Sensitive finding material is never embedded in public evidence.
