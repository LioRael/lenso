# @lenso/api-keys

API Key credentials for application-owned users, service accounts and agents.
No User table, account linking, automatic realm, listener or management entry.

## Entries and ownership

| Entry                                                 | Responsibility                                                |
| ----------------------------------------------------- | ------------------------------------------------------------- |
| `@lenso/api-keys`                                     | Ordinary async credential service, config and Store contracts |
| `@lenso/api-keys/plugin`                              | Thin Lenso lifetime wrapper                                   |
| `@lenso/api-keys/auth`                                | Optional public Auth source, explicit scope profile           |
| `@lenso/api-keys/manage`                              | Opt-in metadata/read/revoke sidecar Operations                |
| `@lenso/api-keys/drizzle/pg`                          | Borrowed native Drizzle PostgreSQL database                   |
| `@lenso/api-keys/drizzle/sqlite`                      | Borrowed native Drizzle Bun SQLite database                   |
| `@lenso/api-keys/drizzle/d1`                          | Borrowed D1 binding, authoritative primary sessions           |
| `@lenso/api-keys/drizzle/schema-pg` / `schema-sqlite` | Credential-owned tables                                       |

The root imports only `node:crypto` and its own core. Auth, Lenso, Manage,
Engine, Zod and Drizzle are optional peers, required only for their entries.
Bun-first, not a browser credential issuer. No blanket Node/Workers/edge
compatibility claim. The D1 Store entry has no Bun imports.

The application owns the Store/database and identity/permission readers.
`close()` rejects new calls and drains current calls; it never closes borrowed
clients. The plugin registers service cleanup immediately. Resources acquired
by the application's `setup(context)` must register their cleanup immediately
with that same context. Do not acquire clients in config module top-level code.

## Independent service

```ts
import { createApiKeys } from "@lenso/api-keys";
import { postgresApiKeyStore } from "@lenso/api-keys/drizzle/pg";

// db and these policy functions belong to your existing application.
const keys = createApiKeys({
  store: postgresApiKeyStore(db),
  config: { maxLifetimeMs: 90 * 24 * 60 * 60_000, maxOverlapMs: 10 * 60_000 },
  authorizeManagement, // validate current trusted caller, target tenant and action
  grantScopes, // independent delegation policy, never echo untrusted scopes
  subjectActive, // current account/agent status by full namespaced reference
  authorizeUse, // current subject permission + membership + actual resource policy
});

const issued = await keys.issue(
  {
    subject: { namespace: "company-accounts", tenantId: "tenant-a", subjectId: "account-17" },
    requestedScopes: ["notes:read"],
    expiresAt: Date.now() + 60 * 60_000,
    requestId: "application-owned-request-id",
  },
  trustedCaller,
);
// Deliver issued.credential only through an authenticated protected channel.
// Later, load the actual note and call:
await keys.use(presentedCredential, "notes:read", note);
// Owner: await keys.close(); database owner separately closes db.
```

`apiKeyConfig()` validates and freezes the two millisecond ceilings. To use
existing Lenso Config, pass the same public `ConfigBinding` as the plugin's
`config` and call `context.config!(binding)` in `setup(context)`. Its schema can
validate application configuration and transform the credential ceilings with
`apiKeyConfig`; sources/resolution remain owned by Lenso Config.
No separate configuration loader or secret environment convention is installed.
Times are safe-integer epoch milliseconds. Each key has a finite expiry.

`authorizeManagement(caller, action, target)` must return exact `true`, after
verifying caller provenance with current Auth `enforce` or the application's
equivalent trusted entry. It receives an immutable target: full subject,
safe key metadata when loaded, and requested scopes for issuance. Query/revoke/
rotate first authorize the target partition, then the stored record. Business
JSON subject fields select a target; they never establish the caller's identity.
The application chooses owner/tenant/admin rules separately for each action.
`grantScopes` can return only a subset of requested scopes. Possessing a scope
does not confer issuance or delegation permission. Scope expansion requires a
new independently authorized issuance; rotation cannot alter scopes or expiry.

## Credential and retry rules

- Credential: `lk_<public UUID>.<32 random bytes as base64url>`.
  CSPRNG: `node:crypto` `randomBytes` and `randomUUID`; SHA-256 digest and
  fixed-size `timingSafeEqual` checks. High-entropy secrets are not passwords.
- The full credential appears only on winning issue/rotate results.
  Store rows hold digests, never raw secrets. Public metadata, actors,
  list/read/verify results and normalized errors contain neither secret nor digest.
  Do not log inputs, successful issuance results or raw provider errors.
- `requestId` is unique within `(namespace, tenantId)`. A repeated issue rechecks
  management/delegation policy, matches subject/scopes/expiry, and returns
  `{key, credential: null, replayed: true}`. Changed parameters conflict.
  This prevents duplicate issuance for the same retained request record, not
  exactly-once execution. Deleted records lose deduplication history.
- Lost winning response means lost secret. There is no secret-recovery API;
  read metadata and explicitly rotate with its current `revision`.
- Rotation requires `expectedRevision` and an explicit `overlapMs`, bounded by
  config. One competing revision wins; stale attempts return `CONFLICT`, never
  the winner's credential. Current and predecessor work until `overlapUntil`
  (exclusive), bounded by original expiry. Zero overlap invalidates the old
  credential immediately. Further rotation is refused until overlap ends.
- Revoke addresses the stable public ID and full subject, revoking both secret
  generations and a concurrently rotated successor. Repeating revoke succeeds
  for the retained record; missing/wrong-subject records return false.
- `verify` authenticates the credential and checks subject activity; it is not
  business authorization. `use` additionally intersects the exact scope ceiling
  with current application authorization. It re-verifies after the policy read.
  There are no wildcard scopes, role snapshots or positive verification caches.

Authoritative reads observe committed revocation on subsequent validations.
Validation/policy reads and a later business mutation are not one transaction;
an already running request is not forcibly cancelled. For sensitive writes,
application-owned transactions/conditional writes must establish the required
business fence. No global strict-consistency guarantee is made.

## Optional Auth source

```ts
import { createAuth, realm, audience } from "@lenso/auth";
import { apiKeySource } from "@lenso/api-keys/auth";

const auth = createAuth(
  realm(
    "company",
    apiKeySource({
      keys,
      realmId: "company",
      requiredScopes: ["notes:read"],
      kind: "service", // choose user/service/guest explicitly for your subject model
    }),
  ),
);
const read = auth.for(audience("notes:read")).memberships(readCurrentMembership);
const actor = await read.required(presentedCredential);
await read.enforce(actor, actualNote, currentNotePolicy);
await keys.use(presentedCredential, "notes:read", actualNote);
// Register auth.close() with its owner as well.
```

Auth creates the actor through public source verification and re-verifies the
original evidence on every `enforce`. No private WeakMap access or forged actor.
The source is not session evidence and cannot satisfy session/MFA requirements.
Current resource/membership policy belongs in `enforce`, not in login success.
Retain the current credential at the trusted service boundary and call `keys.use`
as shown after an asynchronous Auth policy: current Auth has no non-session
post-policy credential hook. `use` runs the configured current-use policy and
rechecks credential state after it. This is not an atomic business-write fence.

**Scope profile is fixed per source.** Current Auth verification context has no
operation audience. Never reuse a read-only profile to authorize write/delegate
audiences: install explicit matching profiles/access boundaries for those
operations and retain the shared service's policies. Profile selection is trusted
assembly, not a caller-provided scope list. A future audience-aware source hook,
if needed, belongs to the Auth integration owner.

Default subject IDs are `key-subject:sha256:<digest>` of the JSON
`[namespace, tenantId, subjectId]` tuple: bounded and collision-resistant, with all
three identity fields included. Alternatively, supply an authoritative
`subjectId(subject)` mapping to a stable ID within Auth's 512-character bound.
Mapping to a session subject requires an explicit namespace-aware identity
lookup, never equality of provider IDs or emails.

API Keys need not have their own realm. For a shared realm, the application can
use public `defineSource` with a discriminated session/key evidence union, dispatch
to its existing session source or the matching key source, and explicitly map
both to the same canonical identity contract. Auth retains the selected evidence
and revalidates that branch; no credential fallback or automatic identity merge.
For isolation, declare separate realms. Realm is authority, not tenant.

## Lenso and optional Manage

`createApiKeyPlugin({id, requires: [database, authPlugin], setup(context) {...}})`
builds the same service. Resolve resources with `context.get(database)` and
`context.get(authPlugin)`, using those exact installed instances, not string DI.
The wrapper opens no database and installs no operations.

`createApiKeyManage(keysPlugin, "key-management")` returns a separate `plugin`,
`operations` and `manage`. Install that exact sidecar and explicitly select
operations for each CLI/MCP/HTTP entry. Its methods receive trusted context
`{caller}` from the current entry binding; JSON caller/approval fields are not
accepted. `canList` must check the current entry's permission independently.
The underlying key service still enforces provenance and object/tenant policy.

Only `list`, `read` and `revoke` are offered. Generic Engine/Manage JSON redaction
is not a secret delivery channel, so issue/rotate are deliberately absent.
Application-owned protected issuance routes call the ordinary service. No Manage
listener or default allowlist, Tasks scheduler, global context or extra logging/
telemetry implementation is installed; selected Operations reuse Engine's
existing telemetry. There is no Tasks job carrying a raw credential or actor.

## Migrations and checks

Explicit baseline SQL (no automatic migrator or Drizzle journal):

- `@lenso/api-keys/migrations/pg/0000_api_keys.sql`
- `@lenso/api-keys/migrations/sqlite/0000_api_keys.sql`

Apply using the application's existing migration owner against authorized test
or application databases. No subject foreign key is required; application-owned
same-database migrations may add one. Never apply these during plugin startup.
For credential-bearing HTTP endpoints, reuse existing Auth Fetch extraction and
explicit ingress/Origin policy. Protect secret delivery with TLS, no-store
responses and application-owned rate limits; never accept a JSON actor.

PostgreSQL rotation locks the row before evaluating database time. SQLite/D1
rotation uses one conditional UPDATE. Select an authoritative database, not a
stale PG replica. D1 adapter details and backend checks are described by its
implementation/test contracts; production replication is not a local test result.
`d1ApiKeyStore(binding)` takes a native `D1Database` binding, not an arbitrary
replica-capable Drizzle session. Every operation creates a fresh
`withSession("first-primary")` session and wraps it in native Drizzle.
The optional `@cloudflare/workers-types` peer supplies its public binding types.
Application and database clocks must agree on epoch time; validation uses the
service clock, while conditional store mutations also enforce database time.

Build Core, Engine, Web, Auth and Manage before this package. Then:

```sh
bun run --cwd packages/api-keys build
bun run --cwd packages/api-keys typecheck
bun run --cwd packages/api-keys test
```

Core/authorization fixtures are unit tests, not provider verification. The
separate Drizzle suites use disposable backend resources and explicitly report
unavailable PostgreSQL binaries. Only test credentials are generated.
The workspace's single Bun lockfile includes this package. Use frozen installs;
database migrations remain explicit application-owned operations.
