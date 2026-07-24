# M6 Incident Map

Version: `m6.v1`. Use the stable issue code from structured evidence; do not
infer recovery from a log substring.

| Issue-code prefix | Evidence | Safe first action | Approval boundary |
| --- | --- | --- | --- |
| `delivery_recovery_` | desired/observed revisions and completed effects | preserve last-valid state and rebuild an idempotent resume plan | environment mutation |
| `failure_` | Failure Scenario observations and cleanup | isolate the failing dependency and keep the declared continue/degrade/reject outcome | destructive cleanup |
| `restore_` | backup, Store, state digests, replay cursors | restore again into an empty passive Store | restore activation |
| `disaster_` | fencing, passive health, RPO/RTO, identity | keep passive non-authoritative and repair the plan | disaster cutover/failback |
| `performance_` | pinned profile, budgets, variance | separate environment drift from product regression | support-envelope change |
| `security_` | threat model, finding, scan, exact subjects | remediate or renew exact risk disposition | accepted risk/release |
| `retirement_` | Consumers, deprecation, replacement, freshness | refresh Consumer evidence | Contract Retirement |
| `ga_` | Support Manifest and exact component set | select a declared combination | none |
| `coordination_` | Data Plane continuity and protected operations | continue established traffic; pause coordinated mutation | coordination resume |

Escalate when authority is ambiguous, evidence is stale, committed effects may
be lost or duplicated, two regions can write, sensitive material is exposed,
or remote release bytes mismatch. Preserve content-addressed evidence and
record cleanup separately from recovery success.
