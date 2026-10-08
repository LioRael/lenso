# @lenso/authorization

A composable authorization engine for trusted application facts. It has no User
table, required organization, HTTP transport, database, authentication method or
Lenso runtime dependency. RBAC is one module, not the engine's only model.

| Entry                                                     | Purpose                                                            |
| --------------------------------------------------------- | ------------------------------------------------------------------ |
| `@lenso/authorization`                                    | Decisions, conditions, scoped RBAC, bounded lists, role management |
| `@lenso/authorization/auth`                               | Adapter to one exact existing Auth `Access.enforce` chain          |
| `@lenso/authorization/plugin`                             | Thin Lenso setup wrapper                                           |
| `@lenso/authorization/drizzle/pg`                         | Borrowed native Drizzle PostgreSQL role store                      |
| `@lenso/authorization/drizzle/d1`                         | Borrowed native Drizzle D1 role store                              |
| `@lenso/authorization/drizzle/sqlite`                     | Borrowed Bun SQLite role store                                     |
| `@lenso/authorization/drizzle/schema-pg`, `schema-sqlite` | Role graph schemas                                                 |
| `@lenso/authorization/migrations/*`                       | Explicit SQL migrations                                            |

The root imports none of Auth, Lenso, Drizzle, Manage, Web, Tasks, Log or OTel.
Optional peers are required only by the selected subpath. No Cedar, OpenFGA or
Casbin dependency is installed. Relation and custom-policy interfaces allow an
application-owned external adapter without copying a policy language.

## Trust and evaluation contract

`Principal` identifies `(realmId, subjectId, kind)`. A realm is a verified
identity-source namespace, not a tenant. Equal emails or subject IDs in different
realms do not link accounts. Explicit application mapping is required.

The pure core accepts facts already trusted by the caller; it **does not
authenticate a JSON principal**. Keep it inside the service, after authentication
and resource loading. Public request DTOs must not supply the principal,
membership, resource scope/owner, credential ceilings or approval facts.

`Request<Action, Resource, Context>` carries the principal (or explicit `null`),
action, resource and context. Core facts must be finite plain data; dates should
be projected to timestamps/strings. Each check copies and freezes facts before
awaiting callbacks. Callbacks are trusted installed code, not a sandbox.

Evaluation order:

1. Validate the action and required identity, realm, operation audience and
   credential presence. Missing required facts deny.
2. Optionally resolve authoritative resource attributes. Its type and ID cannot
   change; scope comes from this resolved object, not an unverified client claim.
3. Intersect any verified credential permissions/expiry with the target, optionally
   resolve trusted attributes, then run all application `boundaries`. No
   allow-producing extension runs before these.
4. Evaluate matching rules and installed policies. Policies return exactly
   `allow`, `deny` or `abstain`. Applicable explicit deny wins; otherwise any
   allow grants; all abstain/no match denies.

Actions, resource types/IDs, scopes and attribute keys match **exactly**.
`scope` is `{type,id}`, with no inferred parent, tenant prohibition or wildcard.
A permission with no `resourceId` explicitly covers every resource of its exact
type in its exact scope. `*` has no special meaning. A rule with no scope
explicitly applies across scopes, so use it deliberately.

Independent allow rules/policies are OR grants. Use `all(...)` for an RBAC grant
AND a resource/request condition; do not install a second allow rule and expect
it to restrict the first. `any(...)` is explicit OR inside a condition. Both
require nonempty children; cycles/depth over 32 are rejected. Every evaluated
branch is checked, so an exception in an OR branch cannot be hidden by true.

Built-in ABAC is limited to own-key primitive `equals`/`in` comparisons on
principal/resource attributes or context. Missing attributes never match, even
against `null`. Trusted predicates can express application-specific logic.
An optional structured `attributes: {resolve(request,evaluation)}` provider
replaces selected principal/resource attribute bags or context with authoritative
finite facts. It cannot change identity, action, scope or credential ceilings,
or mint a principal for an anonymous request. Omitted bags retain supplied facts;
returned bags replace, not merge. Provider exceptions refuse access.
Relations make one direct resolver call per relation condition: no graph
traversal, recursive usersets or relationship database is provided.

Unknown actions/roles, invalid graphs/outcomes, evaluated resolver exceptions and
timeouts cannot yield allow. Default deadline is 1 second; `timeoutMs` supports
1–60,000 ms. The deadline races asynchronous evaluation and checks elapsed time
before allow, but cannot preempt synchronous JavaScript or terminate callback
work. Callbacks must honor `Evaluation.signal`, remain read-only and settle
after cancellation. Late callback success cannot change a returned deny.

`check` always returns a safe `{effect,code}`; `can` returns a boolean; `enforce`
rejects with `AuthorizationError("Access denied.")`. All are asynchronous.
No resource IDs, exception text or rule data enter decisions. Reason codes are
service diagnostics, not a public policy-discovery endpoint: untrusted callers
receive the same generic denial via `enforce`, or a boolean preview via `can`.
Do not serialize distinct check/error categories, backend counts or lookup errors
in a way that distinguishes a hidden resource from an absent one.

## Four minimal recipes

These recipes use the existing Notes domain, not a new demo application.
Executable counterparts are in [test/usage.test.ts](test/usage.test.ts); the
real Auth adapter uses `StoredNote` from the existing Notes example in
[test/auth.test.ts](test/auth.test.ts).

### 1. Pure RBAC, without organizations, DB or Lenso

```ts
import { createAuthorization, rbacPolicy, type RoleGraph } from "@lenso/authorization";

const scope = { type: "personal", id: "home" };
const principal = { realmId: "notes", subjectId: "alice", kind: "user" };
const graph: RoleGraph = {
  roles: [
    {
      id: "reader",
      scope,
      permissions: [{ action: "read", resourceType: "note", scope }],
    },
  ],
  bindings: [{ id: "alice-reader", principal, roleId: "reader", scope }],
};
const authorization = createAuthorization({
  actions: ["read"],
  policies: [rbacPolicy({ actions: ["read"], graph })],
});
await authorization.enforce({
  principal,
  action: "read",
  context: {},
  resource: { type: "note", id: "one", scope },
});
```

Static graph configuration is detached on construction. For editable roles use
`memoryRoleStore(graph, actions)` or a persistent store, and pass `store` instead
of `graph` to RBAC. Roles belong to a scope, not a global string on the user.
Same-scope `inherits: ["reader"]` is supported. Graphs reject cycles, missing
parents, cross-scope permissions/parents, duplicate keys and unknown actions.
Limits are 512 roles, 4,096 bindings and 512 permissions per role.

### 2. RBAC plus resource/request constraints

Replace the standalone RBAC policy with one conjunctive grant:

```ts
import { all, attribute, predicate, rbacPredicate } from "@lenso/authorization";

const authorization = createAuthorization({
  actions: ["read"],
  rules: [
    {
      id: "reader-open",
      effect: "allow",
      actions: ["read"],
      resourceType: "note",
      when: all(
        predicate(rbacPredicate({ actions: ["read"], graph })),
        attribute("resource", "state", "equals", "open"),
      ),
    },
  ],
});
```

Additional realm/audience, approval or credential limits go in `identity` and
`boundaries`, not another allow rule. A platform-scoped administrator role is an
ordinary explicit grant, still subject to all boundaries.

### 3. Organization membership or resource sharing

```ts
import { any, relation } from "@lenso/authorization";

const organization = {
  type: "organization",
  id: "team-a",
  scope: { type: "platform", id: "my-app" },
};
const authorization = createAuthorization({
  actions: ["read"],
  relations: applicationRelations, // { check(principal, relation, resource, evaluation) }
  rules: [
    {
      id: "member-or-shared",
      effect: "allow",
      actions: ["read"],
      resourceType: "note",
      when: any(relation("member", organization), relation("shared-with")),
    },
  ],
});
```

`applicationRelations` reads verified memberships/shares from the application's
existing owner. Explicit resource sharing can permit a legitimate cross-org
operation. If cross-org access needs an approval, require it in a boundary.
No organization module or table is mandatory.

### 4. Custom policy with a credential ceiling

```ts
const authorization = createAuthorization({
  actions: ["read", "write"],
  identity: { credentialRequired: true },
  policies: [
    {
      evaluate: (facts) => (facts.principal?.kind === "service" ? "allow" : "abstain"),
    },
  ],
});
await authorization.enforce({
  principal: verifiedServicePrincipal,
  action: "read",
  resource: loadedNoteResource,
  context: {},
  credential: verifiedReadOnlyCredentialLimit,
});
```

An API key ceiling intersects the current subject's effective permissions; it
does not create roles, bypass revocation or confer Console admission. Independent
service principals are evaluated under their own bindings. A custom policy
cannot bypass a declared credential ceiling. Expiry is checked at the evaluation
snapshot, not guaranteed through a later business write.

## Existing Auth and Notes service

```ts
import { createAuthorizedAccess } from "@lenso/authorization/auth";

const protectedRead = createAuthorizedAccess(
  authentication.for(notesAudiences.read),
  authorization,
  ({ resource: note, membership }) => ({
    resource: {
      type: "note",
      id: note.id,
      scope: { type: "personal", id: note.ownerId },
      attributes: { owner: note.ownerId },
    },
    context: { membership },
  }),
);
// The service loads the actual note, then enforces before returning content.
await protectedRead.enforce(actor, "read", loadedNote, { signal });
```

This is the application-owned replacement for that operation's current service
policy, not an additional route-only check or a parallel authorization truth.
Every call enters the exact Auth `Access.enforce` chain, revalidates credentials
and reads configured membership before projecting facts. Actor identity and
audience only come from its verified callback. Forged/copied actors, another Auth
instance, wrong audience, revoked credentials and missing membership fail.
The adapter detaches cloneable business records and membership before async
projection; `Date` in `StoredNote` is supported, resource handles/functions are
not. The projected core facts still must be finite plain data.

The current public Auth API does not expose verified API-key scopes. Supply
`credential` from an application-owned verified ceiling reader in the facts
callback and set `identity.credentialRequired` when it is mandatory. Do not
infer scopes from JSON or an actor's kind. Extending Auth's verified source
contract belongs to its integration owner.

Auth currently refuses null in `Access.enforce`; this adapter therefore protects
authenticated operations only. An explicitly public anonymous entry may call
the pure engine with `principal:null` and a public-resource predicate. A failed
login is not anonymous fallback. Auth still works unchanged when this package
is not installed. Source/facts callbacks use existing Auth's cooperative signal
contract; the engine timeout does not bound authentication before its callback.

## Console identity choices

- **Shared realm:** business accounts, org memberships and Console admission
  remain separate facts. Explicit `console` instance or platform bindings grant
  admission; org ownership does not. Customer-content read, financial changes and
  impersonation need separate actions, credential ceilings and approval gates.
- **Separate Console SSO/realm:** install a different Auth source/instance and
  audience, then reuse the same core contracts. Map identities only by an explicit
  trusted application mapping. A matching email/subject ID is not a mapping.

Neither deployment dictates shared or separate `users` tables. Platform grants
must deliberately express resource scopes or approved cross-scope policy; there
is no magic global administrator and no `skipAuth`.

## Role administration and optional Manage

Management is **off by default**: no routes, operations or tools are registered.
Construct `createRoleManagement` only with trusted `authorize` and
`grantAuthority` adapters. It provides concrete `createRole` (grant),
`editRole` (edit), `bindRole` (bind), `revokeBinding` (revoke), and
`delegateRole` (delegate). Using a permission grants none of these actions.

`authorize` receives the scope, target ID and immutable proposed role/binding
(including recipient and delegation source). It must use a real Auth actor or
trusted entry credential, compare it to the supplied principal, and run the
same service-side authorization chain. This is not satisfied by accepting
`request.actor` JSON or returning true for a claimed role. Application policy can
forbid self-binding, restrict recipients and require approval for a particular
proposal. Explicit authorized self-binding is not globally forbidden.

`grantAuthority` supplies the verified `permissions`, exact `scopes` and finite
`maxExpiresAt`, intersected with the current credential ceiling and any approval.
All inherited/effective proposed permissions must fit. New bindings require a
live finite expiry. Role edits also check affected descendants and existing
binding expiries, including unbound roles that could stage elevation. An editor
cannot increase their own effective permissions through an already-bound role
or ancestor. Every accepted mutation validates the complete graph and performs
one revision CAS; conflicts are not retried.

The graph is read **before** management authorization so revocation/change in
that same store during authorization loses the final CAS. Authorization from
another store, credential owner, clock or external approval is not atomically
fenced by this CAS; stronger operations require a business-owned transaction or
conditional write at that boundary. Pass a deadline `Evaluation.signal` and a
fresh trusted `now`; do not retain an Evaluation as a permission ticket.

Delegation creates an independent binding capped at issuance by grant authority,
source effective permissions and source expiry. Revoking its source later does
not cascade. Later separately authorized role edits change all active bindings;
issuance permissions are not a permanent delegated snapshot ceiling. Applications
requiring cascading or immutable delegation should reject `delegateRole` and
use a separately reviewed adapter, not assume those guarantees.

Mutation results contain only a revision, never the complete graph. Store reads
and `effective*` helpers are trusted internal APIs, not management list endpoints;
validate graphs before using the helper functions directly.

To enable Manage, wrap selected concrete methods in an application service that
obtains a trusted actor via its existing entry binding. Declare the same shared
schema/service with `defineOperation`, then use existing `defineManage` with the
exact plugin and only those operations. CLI, MCP and agent allowlists are separate;
no business input accepts an actor/grant proof. Reuse Tasks for durable management
work if needed, but its payload must not persist an allow ticket. No public
Auth/Tasks/Manage interfaces are modified by this package.

## Lists, snapshots, revocation and write boundaries

Single-object `check` does not authorize a whole query. `authorizeList` implements
the explicit bounded fallback: pass the **complete** trusted candidate set
(maximum 1,000), shared request facts and `{maxCandidates,offset,limit}`.
It checks every item before pagination, returns only visible items and a visible
total, and refuses oversized sets or any evaluation failure without partial
results. Do not pass one unrestricted backend page, its count/aggregates, or
resource-specific errors to callers. Aggregate only over the authorized result.

This release does not compile rules, arbitrary TypeScript predicates or relation
resolvers into SQL. For unbounded lists, refuse unless the application has an
independently reviewed equivalent database constraint that includes boundaries,
denies and current credentials. A candidate-fetch optimization may narrow a
superset, but does not replace final item checks. UI previews are UX only; Web,
CLI, MCP and agents call the same protected service.

Store-backed RBAC reads on every evaluation; there is no positive cross-request
cache. A caller can explicitly read one `RoleSnapshot` and construct request-local
RBAC with its graph, plus one trusted resource/context snapshot, for repeated
checks. That snapshot remains stale through the rest of that invocation; discard
it afterward. Shared snapshots do not create atomicity across separate resolvers.

Revocation is observed by the next fresh read that sees the committed graph
revision. Existing decisions/in-flight snapshots are not invalidated. Database
replicas, transaction snapshots and adapter caches can delay it; short TTL is not
instant revocation. Policy/graph, identity/membership, credential and resource
versions plus invalidation must be designed before adding a positive cache.

An allow is not a durable capability. Mutable owner/status/balance conditions
need an atomic recheck or version/owner predicate in the actual business write.
[test/toctou.test.ts](test/toctou.test.ts) uses real SQLite to demonstrate a
concurrent owner change making the conditional write affect zero rows.
DB transactions cannot cover external notifications/payments; their effects need
their own reviewed idempotency/fencing/compensation. No exactly-once claim is made.

## Explanation, observation and lifecycle

Ordinary `check` has no rule paths. Configure an explicit `explain` action/resource
gate to enable `explain(managerRequest,targetRequest)`; the manager must be
authenticated at the trusted entry and pass that gate. Its result contains safe
reason codes and ordinal paths such as `rules/0/deny`, not rule IDs, tenant IDs,
predicate details or exception messages. Never expose raw core facts/graphs as
diagnostics. Auth-backed explanation must run within the application's verified
management `Access.enforce` callback, not accept a JSON manager request.

Optional `observe` receives only completed policy decision codes/effects, not
all early identity/error outcomes. It can call existing Log/OTel interfaces;
observer failure/timeout refuses access. Logging is not durable auditing, and
this callback provides no atomic audit/write guarantee.

The core owns no connections, workers or timers beyond per-check deadlines.
Drizzle stores borrow their database and never close it. `createAuthorizationPlugin`
accepts `setup(context)` and exact `requires` instances; setup obtains providers
with `context.get(provider)`. Owned resources must register cleanup immediately
through existing Lenso lifecycle; borrowed Auth/DB instances remain with owners.
Startup configuration can use existing `definePluginConfig`/`bindConfig`; validated
engine options are ordinary TypeScript configuration, not a new config center.

## Persistence and checks

See [Drizzle adapter notes](src/drizzle/README.md) for initialization, database
consistency and migrations. Local checks:

```sh
bun run --cwd packages/lenso build
bun run --cwd packages/auth build
bun run --cwd packages/authorization build
bun run --cwd packages/authorization typecheck
bun run --cwd packages/authorization test
```

The PostgreSQL fixture test is skipped unless an explicitly task-owned local
`authorization_fixture` database is supplied with `AUTHORIZATION_TEST_PG_URL`
and `AUTHORIZATION_TEST_PG_OWNED=1`. Never point it at an existing app/production
database. D1 uses local Miniflare, not a deployed Cloudflare database.
See [VALIDATION.md](VALIDATION.md) for actual results and remaining unverified items.

Design references consulted: [OWASP authorization](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html),
[Cedar terminology](https://docs.cedarpolicy.com/overview/terminology.html),
[OpenFGA concepts](https://openfga.dev/docs/authorization-concepts).
