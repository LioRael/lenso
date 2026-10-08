# Organization

Optional business organizations, memberships and invitations. The root entry is
an ordinary async service with no runtime dependencies on Lenso, Auth, Drizzle,
Manage, Tasks or notification providers.

## Responsibility boundaries

- **Auth** establishes identity and revalidates its proof. A subject is
  `{ realmId, subjectId }`, not a foreign key to a users table or an email address.
- **Organization** persists relationships: `member`, `admin`, `owner`, and
  invitations.
- **Authorization** decides whether a verified subject may perform an action
  within a particular organization. `OrganizationAccess.check` is required.

An organization owner is not a Console or platform administrator. A shared Auth
identity may be a business customer without Console admission, or a Console
member without any business organization. Independent Console SSO can use another
Auth realm/source; this package does not create a second password system.

## Direct service and existing Auth

```ts
import { audience } from "@lenso/auth";
import { createOrganizationService, memberRelationshipPolicy } from "@lenso/organization";
import { createAuthOrganizationAccess } from "@lenso/organization/auth";
import { postgresOrganizationStore } from "@lenso/organization/drizzle/pg";

// Borrow the application's existing Auth and Drizzle database.
const access = authentication.for(audience("business:organization"));
const organizations = createOrganizationService({
  store: postgresOrganizationStore(database),
  access: createAuthOrganizationAccess({
    access,
    policy: memberRelationshipPolicy,
  }),
});

// evidence comes from the trusted ingress, never organization input JSON.
const actor = await access.required(evidence);
const created = await organizations.createOrganization(actor, { name: "Operations" });
const members = await organizations.listMembers(actor, {
  organizationId: created.value.id,
});
```

The Auth adapter uses `Access.enforce` on every call and CAS retry. Cloned,
wrong-audience, wrong-instance and revoked actors fail Auth verification.
`memberRelationshipPolicy` is an **opt-in example policy**, not inherent role
semantics: members can read the organization/members and leave; admins manage
ordinary members and invitations; owners can grant roles/ownership and transfer
their ownership. Creation is allowed to verified subjects under this policy.
Applications should constrain creation, actor kinds, realms and administrative
actions according to their own authorization requirements.

Cross-organization administrators require an explicit policy in the access
adapter, independently verified by the application. No global organization list,
implicit platform override or Console admission rule is provided. All member and
invitation IDs, lists and totals are checked in the supplied organization scope.
Policy callbacks receive actual stored relationships, not client-assigned roles.
Single-invitation actions include the actual scoped invitation and target, without
the digest. Role changes include the requested role. List actions authorize the
**whole organization's** list and count, not a filtered per-invitation view;
policies that permit only individual invitations should deny these list actions.

## Thin Lenso registration

`createOrganizationPlugin` from `/plugin` accepts:

```ts
import { createOrganizationPlugin } from "@lenso/organization/plugin";

const organization = createOrganizationPlugin({
  id: "business-organizations",
  database: databasePlugin,
  access: organizationAccessPlugin,
  store: postgresOrganizationStore, // d1OrganizationStore for D1
  config: { invitationLifetimeMs: 86_400_000 },
});
```

`databasePlugin` and `organizationAccessPlugin` are the **exact installed
instances**; the latter supplies `OrganizationAccess<Actor>`. Install those
instances alongside `organization`. No resources are acquired during config
import, no migrations run at startup, and stopping this plugin does not close
borrowed Auth/database resources.

Factory configuration is validated by `resolveOrganizationConfig`. Applications
using Lenso Config may resolve these values in their existing configured setup
and pass them to `createOrganizationService`; this package does not introduce a
configuration source or hot-reload system.

| Option                 | Default | Allowed                                   |
| ---------------------- | ------- | ----------------------------------------- |
| `invitationLifetimeMs` | 7 days  | 1 second to 30 days                       |
| `maxMembers`           | 1000    | 1 to 1000                                 |
| `maxInvitations`       | 1000    | 1 to 1000, including terminal invitations |
| `conflictRetries`      | 8       | 0 to 32 known CAS misses                  |

## Operations and invitation handling

The service exposes organization create/read/update; member list/add/remove/leave;
role changes; explicit owner grant/transfer; invitation create/read/list/accept/
revoke/prune. Input objects reject unknown fields. Add and invite always assign
`member`; ordinary role input cannot assign `owner`. Self-promotion from member
to admin and self-grant of ownership are rejected even with a permissive checker.

`createInvitation(actor, { organizationId, target })` returns a token once in
`result.value.token`. It contains 256 random bits. Storage contains only a SHA-256
digest bound to the organization, invitation ID, realm/subject, fixed role and
deadline. Do not log the token, put it in a URL or expose it in discovery metadata.
Acceptance requires the exact target identity, organization, invitation ID and
token. It atomically consumes the pending invitation and adds membership.

Replay returns `INVITATION_ACCEPTED`; revoked and expired invitations return
`INVITATION_REVOKED` and `INVITATION_EXPIRED`. Wrong subject/token/scoped ID returns
`NOT_FOUND` after access checks. Removing/leaving a membership also revokes its
pending invitations in the same write. Expiry is derived from storage time;
expired invitations never grant membership.

No email is sent. An application may call its optional Notification adapter
**after** invitation creation commits. Delivery failure must not roll back or
replace the invitation fact; retry delivery separately using the existing Tasks
integration if needed. An uncertain storage write is not automatically retried.

Terminal/expired records remain available until explicitly
`pruneInvitations(actor, { organizationId, before })` removes them. Pruning never
removes a still-live pending invitation. After pruning, replay returns `NOT_FOUND`.

Each mutation returns `{ value, change }`; `change` includes organization ID,
new version, action and affected subject references, never invitation credentials.
`organizationMembershipReader(store)` from `/auth` can feed
`Auth.Access.memberships`, returning role/version or null after removal.
Authorization caches must include organization + realm + subject and revalidate
versions or arrange their own invalidation. Return metadata is not an event
delivery guarantee; no global event platform is installed.

## Storage and explicit migrations

- PG: `/drizzle/pg`, `/drizzle/schema-pg`, `migrations/pg/0000_organizations.sql`.
- D1: `/drizzle/d1`, `/drizzle/schema-sqlite`,
  `migrations/sqlite/0000_organizations.sql`.

Add the chosen schema to the application's existing Drizzle schema. Incorporate
the reviewed SQL into **one** application-owned migration history and apply it
explicitly to an authorized database; do not apply both Drizzle and Wrangler
migration runners to one D1 store.

One row holds each organization's bounded JSON relationship snapshot and version.
Every mutation performs version CAS. PG locks the row inside a transaction and
checks the deadline in a fresh statement after waiting. D1 uses a single
conditional `UPDATE ... RETURNING`, without interactive transactions. Last-owner
checks and all transfer/member/invitation changes are committed together. A known
CAS miss rereads and reauthorizes; a storage exception has unknown outcome.

This design rewrites the snapshot and serializes writes per organization. It is
intended for bounded organizations, not unbounded enterprise directories. A
512,000-byte serialized state limit supplements the count limits. High contention
can return `CONFLICT`; large organizations need a separately designed normalized
store. Direct SQL edits bypass service authorization and are not a public API.
Admission reserves timestamp-growth headroom so revoking many pending invites
cannot prevent member removal/exit. Lowering configured count limits stops growth,
but does not block cleanup of already-admitted relationships.

## Validation

After building existing `@lenso/core` and `@lenso/auth`, run:

```sh
bun run --cwd packages/organization build
bun run --cwd packages/organization typecheck
LENSO_REQUIRE_POSTGRES=1 bun run --cwd packages/organization test
```

PG tests create and stop their own private loopback cluster; the required flag
fails if `postgres`, `initdb` or `pg_ctl` is unavailable. D1 tests use the existing
installed Miniflare/workerd toolchain with disposable local D1 state, not a SQLite
shim. Auth fixture tests exercise the real Auth proof boundary but do not validate
an external identity provider. Local D1 evidence does not validate deployed
Cloudflare replication. No exactly-once or cross-resource consistency is claimed.

The workspace uses the single integration-owned root Bun lockfile.
Manage/CLI/MCP operations are not automatically registered. If an application
exposes any, it must explicitly select methods, authenticate trusted entry context
and apply its organization authorization policy.
