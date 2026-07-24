# M6 Upgrade, Rollback, and Contract Runbook

Version: `m6.v1`. Authority: the exact
`contracts/ga/lenso.ga-support-manifest.v1.json` shipped with the selected
release. Unknown combinations are unsupported.

## Upgrade

Prerequisites:

- exact current and target release, Config Revision, state format, adapter, and
  Contract digests;
- passing `lenso ga support-check`;
- current backup and restore evidence;
- no stale Deployment, Consumer, Workflow, or migration observation.

Plan with `lenso ga service-upgrade --manifest <support-manifest> --input
<upgrade-input> --json`. Verify migration-first ordering, mixed-version window,
API/Worker compatibility, rollback constraints, and zero dry-run effects.

Stop when the combination is unknown, a Contract or Workflow is incompatible,
a migration is unverified, evidence is stale, or the target Store is not
backed up. Apply through the supported deployment path only after its named
approval. Preserve the last valid Config Revision.

Rollback uses the exact prior release, schema, Workflow, Config Revision,
Secret Reference, edge, and adapter evidence. Do not reverse an irreversible
migration. When rollback is unsafe, pause and follow the incident runbook.

Cleanup removes only disposable staging resources and retains plans, receipts,
observations, and Story evidence.

## Contract lifecycle

Compatible additions stay on the current major. Breaking changes use parallel
majors. Inventory every Consumer, verify the replacement, and complete the
deprecation window before building a retirement plan:

```sh
lenso ga contract-retire --input retirement.json --json
```

Active Consumers, missing replacement evidence, stale observations, or an
incomplete window block Retirement. Applying retirement is the
`contract_retirement` Approval Boundary and requires approval bound to the
exact plan digest. Rollback republishes or re-enables the prior Contract only
through a separately reviewed plan; it never rewrites Consumer history.
