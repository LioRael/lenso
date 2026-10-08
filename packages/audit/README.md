# Audit

`@lenso/audit` records who did what to which resource, in which scope, when, and
with what outcome. Log/OTel remains the runtime diagnostic channel. Audit is not
an authorization engine, compliance certification, tamper-proof ledger, or a
guarantee of lossless delivery.

## Entries and ownership

| Entry          | Purpose                                                        | Additional packages       |
| -------------- | -------------------------------------------------------------- | ------------------------- |
| `@lenso/audit` | Ordinary async service and contracts                           | None                      |
| `/auth`        | Revalidate an actual Auth actor and exact scope policy         | Auth (type-only)          |
| `/plugin`      | Thin exact-instance registration and page-size Config contract | Core, Zod                 |
| `/sqlite`      | Drizzle Bun SQLite or D1 repository and schema                 | Drizzle                   |
| `/postgres`    | Drizzle Bun SQL PostgreSQL repository and schema               | Drizzle                   |
| `/diagnostics` | Bounded OTel failure counter and supplied logger               | OTel API                  |
| `/manage`      | Explicit query-only companion, no automatic entry              | Core, Engine, Manage, Zod |
| `/tasks`       | Existing Tasks reconciliation registration                     | Tasks, Zod                |

Root imports do not load optional integrations. Install only the peers for entries
you use. Repositories borrow databases and never close them. Use the existing DB
resource plugins to own connections; they register cleanup during setup. Startup
does not run migrations. Drain application calls before stopping their DB owner.

## Ordinary service

```ts
import { createAuditService } from "@lenso/audit";
import { createAuthAuditAuthority } from "@lenso/audit/auth";
import { createSqliteAuditRepository } from "@lenso/audit/sqlite";
import { createAuditReporter } from "@lenso/audit/diagnostics";

// db, access and logger are existing, explicitly supplied application instances.
const audit = createAuditService({
  repository: createSqliteAuditRepository(db),
  authority: createAuthAuditAuthority(
    access,
    ({ principal, scope }) =>
      principal.kind === "user" &&
      scope.tenantId === null &&
      scope.scopeId === `owner:${principal.subjectId}`,
  ),
  summaryPolicy: { "notes.remove": { removed: { type: "boolean" } } },
  report: createAuditReporter({ logger }),
});

// actor comes from access.required(trustedRequestEvidence), never business JSON.
const status = await audit.appendBestEffort(
  {
    id: crypto.randomUUID(),
    occurredAt: Date.now(),
    scope: { tenantId: null, scopeId: `owner:${actor.subjectId}` },
    action: "notes.remove",
    target: { type: "note", id: noteId },
    result: "success",
    reasonCode: "removed",
    summary: { removed: true },
  },
  actor,
);
```

`AuditAuthority.resolve` is the trusted server boundary: it must authenticate,
revalidate and authorize the requested `append` or `query` scope, returning only
an identity snapshot. The Auth companion calls the existing `Access.enforce`,
so copied actors, another audience/runtime and revoked sessions fail. Custom
authorities can use a server-owned system principal and explicitly return
`{kind:"system", systemId:"maintenance"}`; missing identity is an error, not a
fabricated user. Do not implement an authority by trusting a JSON actor or
unconditionally accepting an arbitrary caller object.

Events require a stable lowercase UUID, server-supplied occurrence time
(integer milliseconds), action, target, result and reason code. Recording time
comes from the service clock. They optionally carry correlation and a relation.
Subject, target, scope and correlation IDs must be opaque identifiers, not
credentials, signed URLs, email addresses or request content. Business identifiers are
bounded ASCII (`A-Z`, `a-z`, digits, `.`, `_`, `:`, `/`, `-`), with no URL scheme.
Targets permit 256 characters; reason codes 64. Trusted Auth identity snapshots
retain Auth's own bounded realm/subject grammar (256/512 characters), including
opaque subject IDs such as `auth0|alice` and issuer URI realms. Map such subjects
to an application-owned scope identifier if they are not scope-token compatible;
do not rewrite or hash credentials to manufacture an audit identity.

Summaries default to empty. Each action explicitly whitelists at most 16 fields
of boolean, bounded integer or fixed enum values (up to 32 literals of 64
characters). No free-form text, nested input, full body, credential hash/digest,
or arbitrary error text is accepted. Credential/body/PII-shaped field names are
rejected even when configured. Applications must still choose safe enum literals
and identifiers: validation cannot recognize a secret disguised as an opaque ID.
Unknown fields, missing tenant declaration, malformed actors and oversized data
fail explicitly rather than silently disappearing.

## Queries, corrections and duplicates

Every `get({scope,id}, principal)` and `query({scope,...}, principal)` requires
an explicit exact scope, including for administrators. Public resources use
`tenantId: null`; tenant-bound resources require their actual tenant. Neither
missing tenant nor missing scope means all resources. The authority must enforce
actual membership/permission, not a client-provided tenant assertion.

Query filters are action, exact target, result, correlation and inclusive
recording-time bounds. Pages use descending `(recordedAt,id)` keyset cursors.
Default limit is 50, configurable maximum defaults to 100 (hard ceiling 500).
No global count is returned. A cursor is only a position within the requested
authorized query, not a permission token or snapshot; concurrent appends or clock
changes can change later pages. Do not use pagination as a complete frozen export.

The repository key is `(tenant namespace, scopeId, id)`, including a distinct
namespace for null tenants. Repeating identical immutable content returns
`duplicate` and the original recording time; changed content at that key fails
`duplicate-conflict`. There is no cross-scope ID-existence oracle.

The service and repositories have no update/delete API. Append a new event with
`relation: {kind:"correction", eventId:originalId}` to correct an existing event
in the same scope. Results use `kind:"outcome"` to link to an intent for the same
action and target. This does not prevent DB administrators, other SQL writers,
backups or retention workflows from changing/removing underlying data.

## Strict versus best-effort

- `append` propagates safe persistence errors; a failed acknowledgement may
  mean the row was committed. Retry only the **same event ID and content**.
- `appendBestEffort` requires a diagnostic reporter. A storage failure produces
  `status:"unconfirmed"` and a safe failure metric/log; it does not assert the
  row is absent. Authorization, malformed data and duplicate conflicts still
  fail. The supplied reporter must have a working sink; reporter failures are
  not swallowed. `/diagnostics` requires a logger and emits only fixed
  mode/stage/code labels, never actor, tenant, target, summary or driver errors.
  OTel exporters/SDK ownership stay with the existing application.
- `prepare` is the strict pre-effect gate. It is disabled unless the repository
  owner explicitly attests `durableIntents:true`. A newly acknowledged intent
  returns `ready` with a service-issued receipt. Duplicate intent returns
  `already-recorded`, **never permission to repeat the business effect**.

```ts
const prepared = await audit.prepare(intentEvent, actor); // result:"intent"
if (prepared.status !== "ready") {
  // Consult the stored outcome/reconcile. Do not rerun the effect.
  return { state: "pending-reconciliation", intentId: prepared.intentId };
}
// Business authorization is still required at the real effect boundary.
const result = await alreadyAuthorizedEffect();
await audit.complete(prepared.receipt, {
  id: stableOutcomeId,
  occurredAt: Date.now(),
  result: "success",
  reasonCode: "completed",
});
```

The application owns effect classification. A rejected/timeout external call
may already have taken effect: append `unknown`, not an invented failure/rollback.
If `complete` cannot confirm the result, it throws `AuditOutcomeUnknownError`
with the persisted intent ID. The effect is not rolled back, automatically retried
or hidden by a successful response. Outcome recording uses the receipt's trusted
pre-effect identity snapshot, including if a session is revoked after the effect.
Receipts are instance-local, not serializable authorization grants.

`durableIntents` is a deployment assertion, not a capability detected by
Drizzle. Do not enable it for memory-only SQLite, uncommitted transaction handles,
asynchronously replicated acknowledgement paths, or unverified durability
settings. PostgreSQL requires an acknowledged commit with appropriate deployment
durability; SQLite requires a persistent database and deliberate sync settings;
D1 requires the actual binding's committed write semantics. A timed-out write
does not authorize continuing. No adapter combines independent business/Audit
writes into a transaction. PostgreSQL transactions and D1 batch atomicity apply
only when the application really submits both operations together through that
existing mechanism; this package does not provide that integration.

For durable supplementary work, `/tasks` defines a reconciliation task carrying
only `{scope,intentId}`. Its application-owned principal is reauthorized; the
resolver reads the real effect, then appends a linked outcome with a stable ID
and occurrence time. It must not repeat the effect. Install/enqueue/drain it
through existing Tasks. Enqueue after an effect is another independent write:
if enqueue fails or the process crashes first, scan persisted intents using an
authorized application workflow. There is no automatic outbox or exactly-once
claim, and no durable actor/credential in the task payload.

## Optional Lenso and Manage wiring

`createAuditPlugin({id,repository,authority,diagnostics?,summaryPolicy?,config?})`
requires the **exact supplied plugin objects**. Config validates only
`maxPageSize`; trusted policy/providers remain code, not string DI or a global
Context. It creates no client, listener, SDK or queue worker.

`createAuditManage({id,audit})` returns a query sidecar, its one Operation and
Manage declaration. Creating it opens nothing. Install the sidecar and explicitly
select its operation for the chosen entry. Supply per-call trusted binding
`context:{principal:actualActor}` plus current-identity `canList`, then let the
service check scope. Default CLI/MCP/agent lists contain no Audit operations.
No append, complete event input, credentials or full request body is handed to
an agent. Query results still contain identities and resource IDs; select this
operation only for callers authorized to read that audit data.

The existing Notes factory accepts optional `audit` in
`createNotesOperations({notes,authentication,audit})`. Only its real `remove`
management operation records a best-effort event; Notes still performs Auth and
owner checks. Auth denials retain a known actor with a fixed reason code;
unclassified business-write errors are recorded as `unknown`, never a claimed
rollback. Assembly opts in to `notes.remove`'s `removed:boolean` summary and
owner scope as above. Omitting Audit preserves the existing graph. The focused
consumer test composes the existing Notes/Auth/DB/Manage services, not another
demo or authorization framework.

## Migrations and provider evidence

Apply `migrations/pg/0000_audit.sql` or `migrations/sqlite/0000_audit.sql` through
the application's explicit migration history before starting the consumer.
Exported `auditPostgresSchema`/`auditSqliteSchema` match those tables and index.
SQLite SQL is also intended for D1's normal migration runner; do not mix runner
histories. The repository uses explicit composite-conflict `DO NOTHING`,
`RETURNING`, and a fresh exact-scope duplicate read. It uses no update,
interactive D1 transaction, invented conditional-write abstraction or batch
rollback promise.

Official references checked against the pinned Drizzle 0.45.3 APIs:
[Drizzle insert](https://orm.drizzle.team/docs/insert),
[Bun SQLite](https://bun.sh/docs/api/sqlite),
[Bun SQL](https://bun.sh/docs/api/sql),
[PostgreSQL INSERT](https://www.postgresql.org/docs/current/sql-insert.html),
[SQLite RETURNING](https://www.sqlite.org/lang_returning.html),
[D1 database API](https://developers.cloudflare.com/d1/worker-api/d1-database/).
D1 `batch` is sequential and rollback-on-statement-failure; it does not cover
external effects. Local workerd tests do not verify cloud replication or
production crash durability.

Focused checks:

```sh
bun run --cwd packages/audit build
bun run --cwd packages/audit typecheck
LENSO_REQUIRE_POSTGRES=1 bun test packages/audit/test
```

Build Core/Auth/DB/Engine/Manage/Tasks and other consumer dependencies first,
because workspace public exports resolve to `dist`. PostgreSQL tests create
their own disposable loopback cluster, never use an inherited database URL.
D1 tests use owned local Miniflare/workerd bindings. Fault-injection tests verify
failure protocol only, not a real provider's durability. See `CHECKS.md` for
actual results and unverified boundaries.
