# Qualification status

This directory is Lenso's canonical current-status source for feature and
qualification evidence. It complements ADRs: an ADR records a decision, while
this ledger records what exact source, artifact, and target combination have
evidence today.

The machine-readable source is
[qualification-status.json](qualification-status.json). Its required shape is
declared by [qualification-status.schema.json](qualification-status.schema.json).
The repository-owned semantic check rejects duplicate records, duplicate exact
qualification tuples, and repeated infrastructure roles in the checked-in
ledger. Do not derive current maturity from prose in an ADR, issue, release
note, or an implementation companion.

[Cross-repository release evidence](release-cohorts.md) defines the separate
exact dependency cohort policy. A cohort proves the source-and-artifact closure
for a distribution candidate; it does not turn a local install into target or
production qualification.

[Cross-repository qualification links](cross-repository-status-links.md)
defines the one-link README migration that keeps Runtime, Web, Auth, Console,
and Agent source documentation from becoming conflicting status boards.

Use the [execution target capability matrix](../architecture/execution-target-capability-matrix.md)
for early Driver and Adapter admission facts. The matrix never replaces an
Environment-plus-Infrastructure qualification record.

## Independent evidence facets

Each record keeps these facets independent:

| Facet | Required evidence | It does not establish |
| --- | --- | --- |
| Design | Immutable decision-source reference. | Source implementation, artifact release, or runtime exercise. |
| Implementation | Exact source revision and paths. | Release, target exercise, or production operation. |
| Released | Exact artifact/version evidence plus an externally verifiable publication receipt. | That any particular target combination was exercised. |
| Local qualification | Exact local Environment and Infrastructure combination with reproducible evidence. | A deployed target or production condition. |
| Target qualification | Exact non-production or isolated target combination with deployment/interaction evidence. | Production traffic, capacity, cost, or incident readiness. |
| Production qualification | Exact production combination with separately governed operational evidence. | Other environments, stores, adapters, or later source revisions. |

Released is not a step in a cumulative ladder. For example, a Workers plus D1
target record does not qualify Workers plus Hyperdrive plus PostgreSQL; both
records may name the same Capability and source revision but remain distinct.

The machine-readable names are design.status set to designed,
implementation.status set to implemented, release.status set to released, and
qualification.level set to local, target, or production. This spelling keeps
the six familiar labels without falsely turning them into one ordered state.

No renderer may turn a ledger entry into a bare claim that a Capability is
supported. It must name the exact qualified combination and link its evidence.

The records value is an object keyed by stable record ID, rather than an array.
That makes one canonical record per ID. Qualifications remain an array because
one subject can have several source-and-target combinations; the semantic check
canonicalizes source references and infrastructure role order, then rejects a
duplicate subject, source, level, Environment, and Infrastructure tuple.

## Required qualification tuple

Every qualification record names:

    subject
    source revision
    execution environment
    infrastructure implementation set
    evidence
    known limitations

The source revision is an immutable 40-character Git SHA. The Environment is
one of Native, Cloudflare Workers, or Simulated. Infrastructure is an explicit
role and concrete implementation, not a generic platform label.

An empty qualification list means no combination is currently recorded. It is
not a negative compatibility assertion. A conflicting record preserves the
source references and makes no implementation, release, or qualification
claim until its owner resolves the evidence.

## Task 1 focus inventory

Task 1 requires a stable record for each target-sensitive focus below. A focus
with no immutable owner evidence is represented by an explicit
`not_assessed` record rather than being absent from the ledger. That absence of
evidence must never be rendered as a negative compatibility result or as an
implicit support claim. Read the machine record for its current independent
design, implementation, release, and qualification facets.

| Required focus | Canonical record |
| --- | --- |
| Workers HTTP | `lenso.web.workers-http` |
| Workers Stream | `lenso.web.workers-stream` |
| Workers WebSocket | `lenso.web.workers-websocket` |
| Workers Runtime | `lenso.runtime.workers-runtime` |
| W01 | `lenso.runtime.workers-w01` |
| W02 | `lenso.runtime.workers-w02` |
| Auth D1 | `lenso.auth.d1-storage` |
| Auth PostgreSQL | `lenso.auth.postgresql-storage` |
| Workers plus D1 | `lenso.auth.workers-d1` |
| Workers plus Hyperdrive plus PostgreSQL | `lenso.auth.workers-hyperdrive-postgresql` |
| Browser Runtime | `lenso.runtime.browser` |
| WASIp2 | `lenso.runtime.wasip2` |
| Bun | `lenso.runtime.bun` |
| Process | `lenso.runtime.process` |
| Remote Adapter | `lenso.runtime.remote-adapter` |

The retained `lenso.auth.workers-g4` entry is deliberately narrower than the
general Auth D1 and Workers plus D1 inventory entries: its existing evidence
only describes the exact isolated G4 combination named in that record.

## Owner update rules

The repository that owns a Driver, Execution Adapter, Plugin, or target
deployment owns its source and operational evidence. This ledger may reference
that immutable evidence without taking ownership of its code or infrastructure.

When adding or changing a record:

1. Link exact source paths at an immutable revision.
2. Use the stable record ID as the records-object key; do not repeat it inside
   the record.
3. Add an independent `Released` entry only when the artifact/version can be
   verified against that revision or its documented build closure **and** the
   entry retains an externally verifiable publication receipt. A local package
   archive or clean-room pass is only release-ready evidence.
4. Add a qualification only for the exact Environment and Infrastructure set
   exercised by the cited evidence.
5. Give every infrastructure role one entry and do not rely on array order.
6. State the remaining limits in the qualification and record-level
   limitations.
7. Use a conflicting implementation facet rather than choosing between
   contradictory historical claims without new evidence.

When a source comes from a retained exact dependency cohort, use the optional
`cohort` source-reference field and run the cohort check as part of this
ledger's validation. A working-tree cohort can support a local record only. An
immutable `release-ready` or `published` cohort may support target or
production source linkage only alongside its exact deployment or operational
evidence. A `Released` ledger facet requires a `published` cohort when one is
referenced, plus the ledger entry's own external publication receipt.

For this directory's focused local syntax check:

    jq empty docs/qualification/qualification-status.schema.json
    jq empty docs/qualification/qualification-status.json
    node scripts/check-qualification-ledger.mjs
    node --test scripts/check-qualification-ledger.test.mjs
    node --test scripts/release-cohort.test.mjs

The jq commands prove JSON syntax. The repository-owned semantic check enforces
the canonical uniqueness rules without a network download. A full JSON Schema
validator may additionally validate the instance against the schema, but is not
required for this documentation-only change.
