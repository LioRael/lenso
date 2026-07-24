# M6 Failure, Backup, Restore, and Disaster Runbook

Version: `m6.v1`. Observed RPO, RTO, and performance values apply only to the
pinned evidence environment.

## Delivery and runtime failure

Collect the exact desired state and an independently sourced observed revision.
Classify the result as continue, degraded, paused, rejected, fail-closed,
partial, or blocked. Preserve the last valid Config Revision, completed
Migration receipts, Inbox/Outbox cursors, Workflow history, and Story evidence.

Pre-apply validation, trust, policy, compatibility, configuration, freshness,
or approval failures must have zero infrastructure effects. A partial
Migration resumes only remaining receipt-backed steps and must not overwrite a
newer observed revision.

Stop on `delivery_recovery_*`, `failure_*`, `migration_*`, `observation_stale`,
or `coordination_unavailable` until the named remediation is complete.

## Backup and restore

Backups bind Service, Store, schema, release, Config Revision, point in time,
snapshot, state partitions, Inbox, Outbox, Workflow timers, Stories, and an
opaque encryption-key reference. Partial backups are ineligible.

Restore only into an isolated empty Store. Verify every state digest and resume
Inbox/Outbox at the first sequence after the checkpoint. Keep authoritative
Workload count at zero throughout verification.

Restore, destructive cleanup, key changes, and activation are Approval
Boundaries. A passing `lenso.service-restore-evidence.v1` artifact does not
grant authority.

## Active-passive disaster recovery

1. Restore and verify the passive region.
2. Verify exact release, configuration, Contracts, Workload Identity, health,
   replay cursors, and support combination.
3. Fence all primary writers.
4. Build the content-addressed disaster recovery plan and failback steps.
5. Stop at `single_region_disaster_cutover`.
6. After named approval bound to the plan digest, grant passive authority and
   switch traffic through the normal environment adapter.
7. Verify zero lost or duplicated committed effects and record observed RPO/RTO.

Failback requires re-seeding the former primary, verification, fencing of the
current authority, a fresh plan, and separate approval at
`single_region_disaster_failback`. Never run both regions as authoritative.
