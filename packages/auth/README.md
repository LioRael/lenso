# @lenso/auth

Authentication and service authorization without a required User table or a
Better Auth dependency. Applications own their accounts, organizations and
objects. Auth refers to them by a stable string `subjectId` within a configured
realm, and owns only the session state it actually manages.

## Entry points

| Import                                            | Responsibility                                                               |
| ------------------------------------------------- | ---------------------------------------------------------------------------- |
| `@lenso/auth`                                     | Sources, trusted actors, realms, operation audiences, requirements, policies |
| `@lenso/auth/plugin`                              | Public Lenso lifecycle integration                                           |
| `@lenso/auth/sessions`                            | Optional opaque managed sessions and domain Store contract                   |
| `@lenso/auth/session-source`                      | Existing `getSession` bridge; external session ownership                     |
| `@lenso/auth/fetch`                               | Explicit credential extraction, origin gate, safe error responses            |
| `@lenso/auth/orpc`                                | Typed middleware for oRPC 2.0.0-beta.42                                      |
| `@lenso/auth/drizzle/pg`                          | Native Drizzle PostgreSQL session store                                      |
| `@lenso/auth/drizzle/sqlite`                      | Native Drizzle Bun SQLite session store                                      |
| `@lenso/auth/drizzle/d1`                          | Native Drizzle D1 session store, no Bun runtime imports                      |
| `@lenso/auth/drizzle/schema-pg` / `schema-sqlite` | Auth-owned session schemas                                                   |

The root imports none of Lenso, oRPC, Drizzle or Bun. Those dependencies are
optional peers selected by entry point. Build uses shared chunks so all public
entry points use the same `AuthError` class.

## Verify a source, protect a service

`defineSource()` preserves the evidence and subject types. A source is trusted
installed code: it must verify credentials using a supported identity API, not
decode a token or accept an input `userId`. In this example, `verifyCompanyLogin`
and `findEmployee` are application-owned functions using the existing identity
system and Drizzle tables.

```ts
import { audience, createAuth, defineSource, realm, type ActorOf } from "@lenso/auth";

const source = defineSource({
  async verify(evidence: CompanyLoginEvidence, { signal }) {
    const proof = await verifyCompanyLogin(evidence, signal);
    const employee = await findEmployee(proof.issuer, proof.subject);
    if (!employee || employee.disabled) return { status: "rejected" };
    return { status: "verified", subjectId: employee.id };
  },
});

const auth = createAuth(realm("employees", source));
const read = auth.for(audience("notes:read"));

async function readNote(actor: ActorOf<typeof read> | null, id: string) {
  const note = await loadNote(id);
  await read.enforce(
    actor,
    note,
    ({ principal, resource }) => principal.subjectId === resource.ownerId,
  );
  return publicNote(note);
}

const actor = await read.required(loginEvidence, { signal });
await readNote(actor, noteId);
// The owner of this standalone instance calls await auth.close().
```

Actors are frozen safe projections containing only `realmId`, `subjectId`,
`audience` and `kind` (`user`, `guest`, `service`). Their type has a private brand;
the runtime also checks exact object provenance. JSON, object spreads, actors
from another Auth instance and actors for another audience cannot authorize.
This prevents accidental trust of DTOs; it is not a sandbox for malicious
installed code. Cross-process callers must present credentials again.

Realm identifies the configured trust authority, not a tenant. The source owns
issuer/key/token-profile validation. A JWT's external `aud` is not the local
operation audience. Local audiences match exact identifiers, without wildcard
rules or a string-to-function registry.

### Optional authentication and errors

`optional(evidence)` returns `Actor | null`; `required(evidence)` returns `Actor`.
Only source outcome `absent` permits null. `rejected`, `unresolved` and malformed
verified results fail with `UNAUTHORIZED`. Persistent guest identities are
explicit verified sources with `kind: "guest"`, not a fallback for failed login.

`enforce()` verifies the actor's provenance, revalidates its source credential,
reads current membership if configured, and runs the service policy. There is
no Request-only cache or durable authorization snapshot. Service callers must
obtain a new actor after their invocation's signal is aborted.

Only exact `true` grants access. Known Auth errors are normalized to safe
messages; unknown provider, membership or policy failures become
`SERVICE_UNAVAILABLE`. Cancellation retains its signal reason.

## Current membership and per-entry requirements

Use your existing membership tables directly. A reader returns a typed grant or
null for no active membership; infrastructure errors must throw rather than
pretend the subject is not a member.

```ts
const read = auth
  .for(audience("notes:read"))
  .memberships(async (subject, note: Note) =>
    findActiveMembership(subject.realmId, subject.subjectId, note.tenantId),
  );

await read.enforce(
  actor,
  note,
  ({ principal, resource, membership }) =>
    membership.role === "editor" && principal.subjectId === resource.ownerId,
);
```

Load the actual object before choosing its tenant. Client `tenantId`, session
`activeOrganizationId`, role claims and login success are not membership proof.
Auth does not require a role model or create a membership table.

Use `requireSession()` with `authoritativeSession()`,
`sessionCreatedWithin(ms)`, `authenticatedWithin(ms)` and
`requireAssurance("mfa")`. Repeated requirements intersect: shorter age, combined
assurance, and authoritative reads cannot be relaxed by a later view.

The source must advertise corresponding capabilities and return verified
evidence. It receives `context.authoritative` when a cache bypass is requested.
Unsupported capabilities fail when constructing the view, normally in setup.
Missing/stale evidence returns `REAUTHENTICATION_REQUIRED`. Session creation
does not prove recent password or MFA authentication; assurance labels are
meaningful only when the configured source truly verifies them.

Business authorization stays in the service even if an HTTP entry adds stricter
requirements. Authentication checks and a later mutation are not automatically
one database transaction: sensitive writes need business-owned conditional
updates or transactions, especially across databases.

## Managed sessions without a User table

```ts
import { createAuth, realm } from "@lenso/auth";
import { createAuthPlugin } from "@lenso/auth/plugin";
import { createManagedSessions, sessionLifetime } from "@lenso/auth/sessions";
import { postgresSessionStore } from "@lenso/auth/drizzle/pg";

const authPlugin = createAuthPlugin({
  id: "employees-auth",
  requires: [database],
  setup(ctx) {
    const db = ctx.get(database); // native Drizzle DB, borrowed
    const sessions = createManagedSessions({
      realmId: "employees",
      login: employeeLoginSource,
      store: postgresSessionStore(db),
      lifetime: sessionLifetime({
        idle: 30 * 60_000,
        absolute: 7 * 24 * 60 * 60_000,
        renewAfter: 5 * 60_000,
      }),
      subjectActive: (id, { signal }) => employeeIsActive(db, id, signal),
    });
    ctx.onCleanup(() => sessions.close());
    const auth = createAuth(realm("employees", sessions.source));
    return {
      ...auth,
      issue: sessions.issue,
      touch: sessions.touch,
      renew: sessions.renew,
      revoke: sessions.revoke,
    };
  },
});
```

`issue()` accepts login evidence, not an unchecked subject. `subjectActive`
must return exact true on issuance and every use, allowing the application to
disable its own employee/account without a shadow User table.

The client gets `sessionId`, `credential`, and the current `expiresAt`.
The credential is a UUID locator plus 32 random bytes; only its SHA-256 digest
is persisted. WebCrypto provides randomness and hashing. Raw credentials never
enter an actor or record. Serve them only through an authenticated, protected
login channel; never put them in logs or URLs.

Operations:

- `source.verify(token)` is read-only.
- `touch(token)` explicitly advances activity without rotating the credential.
- `renew(token)` respects the renewal interval and atomically rotates the
  credential. The old credential stops working; a concurrent stale writer loses.
- `revoke(token)` requires possession, including when the subject is disabled.
  It revokes the stable session ID, including a concurrently rotated successor.
- `close()` rejects new work, signals cancellation and drains in-flight work.
  Store/client ownership remains with its resource owner.

Absolute expiry and idle timeout cannot exceed stored ceilings; renewal cannot
become more frequent. Current configuration can further restrict them. Read-only
validation does not persist configuration narrowing; successful touch/renew
freezes tighter limits. Loosening configuration never exceeds the bounds already
stored, but can remove a temporary read-only restriction within those bounds.

Sources, status readers and Store operations must settle after cancellation for
shutdown to finish. A callback must not await its own owner's `close()` promise.
Cancellation does not roll back a committed database change.
No password implementation, login UI, implicit renewal, session metadata bag,
role snapshot or automatic account linking is installed.

## Drizzle schemas and migrations

Auth owns `auth_sessions` only, keyed by `(realm_id, id)`, with a unique token
digest and a realm/subject index. `subject_id` refers to your stable application
ID. There is no User table or mandatory foreign key; optional same-database
foreign keys belong to application integration migrations.

Apply the selected reviewed script explicitly:

- `@lenso/auth/migrations/pg/0000_auth_sessions.sql`
- `@lenso/auth/migrations/sqlite/0000_auth_sessions.sql`

These are baseline SQL scripts, not an automatic migrator or a Drizzle journal.
Use one application-owned migration runner/history. Startup never creates tables
or applies migrations. Select different physical databases for independent Auth
instances; same-database instances are partitioned by realm.

`SessionStore` is a session-specific contract, not an ORM. Mutation must atomically
check revision, digest, revocation, frozen and effective lifetime bounds, and
renewal interval at the store's current clock. SQLite/D1 use one conditional
UPDATE with RETURNING. PostgreSQL locks the row inside a transaction before
evaluating the database-time predicate in the UPDATE: merely using
`clock_timestamp()` in a statement started before a lock wait is insufficient.
Reads must use the current authoritative state, not an
application cache or stale replica; select the appropriate D1 binding/session
consistency policy when using replication.

Local adapter tests exercise Bun SQLite and a typed SQLite-backed D1 binding.
The existing Notes Worker also runs in actual local workerd/D1; it is not the
mock binding. PostgreSQL integration tests start private real clusters and prove
the lock queues, renewal winner, both revocation orderings and expiry across
lock waits. The tested PG driver is Bun SQL. Run
`LENSO_REQUIRE_POSTGRES=1 bun test packages/auth/test/postgres.test.ts` to forbid
missing-binary skips. Production Cloudflare replication and other PG drivers
are not covered by these checks.

## Fetch, oRPC and existing sessions

```ts
import { os } from "@orpc/server";
import { bearerEvidence } from "@lenso/auth/fetch";
import { requiredAuth } from "@lenso/auth/orpc";

const protectedBase = os.$context<{ request: Request }>().use(requiredAuth(read, bearerEvidence));
// context.actor is non-null; optionalAuth(read, bearerEvidence) is nullable.
// Call the same readNote service; do not move its authorization into middleware.
```

Credential selection is explicit. `bearerEvidence` reads Bearer only, rejects
malformed Authorization, and never falls back to a cookie. `headersEvidence`
copies headers for a session source. `requireSameOrigin(request, origin)` rejects
safe methods, missing/mismatched Origin and cross-site cookie writes; it is an
explicit gate, not an automatically mounted HTTP security layer. Login/renew
handlers must preserve provider response headers/Set-Cookie, configure secure
cookies and protect their own routes. Auth installs no listeners or routes.

`actor` is a reserved middleware context key. Do not pass an existing actor via
untrusted context; authentication middleware replaces it with a verified result.
Non-Auth business exceptions remain business exceptions. Raw Fetch can use the
same access methods and `authErrorResponse()`. CLI wrappers validate credentials
and invoke the same service; they never deserialize a trusted actor from JSON.

`sessionSource({ getSession, subjectId, hasCredential?, session?, capabilities? })`
integrates an existing session API without importing Better Auth. `getSession`
receives headers and a verification context; its wrapper owns any supported cache
bypass/read-only configuration. Without `hasCredential`, null is `unresolved`,
not anonymous. With a reliable source-specific presence check, absent returns
null and a presented credential with a null session rejects. Presence does not
verify cookie signatures. A non-cancellable API is checked before/after but
cannot be interrupted by this adapter.

External session ownership is exclusive: replacing a Lenso `SessionStore` does
not alter another library's signing, storage or revocation. If that library
requires its own User table, that requirement stays inside that optional
integration, not in the Auth domain model.

No old Rust wire/data compatibility, automatic existing-session migration,
cross-process signed delegation, JWT/OIDC implementation or cookie issuance is
claimed by this release.

## Checks

Build Lenso/Web dependencies before consumers, then run
`bun run --cwd packages/auth build && bun run --cwd packages/auth test`.
The tests cover both source behavior and public export integration. The workspace
task graph builds Auth before its checks; tests never clean/rebuild dist while
other packages are consuming it.
`bun run --cwd packages/auth typecheck` includes compile-only checks for branded
subject inference, membership inference, audience separation and optional/required
middleware narrowing. No publication or repository split is required to use it.
