---
status: accepted
---

# Separate Access Control from Organization ownership

Lenso models role-based access control as an independent `access-control`
Plugin rather than behavior owned by the Organization Plugin. Organization
owns Organization identity, lifecycle, membership eligibility, ownership, and
invitation facts. Access Control owns scoped role definitions, permission
grants, subject-to-role bindings, policy revisions, and role-based access
decisions. Removing Access Control therefore removes role policy and its
administration while leaving Organizations and their membership directory
intact.

Access Control addresses scopes through opaque `{ kind, id }` references. A
scope-owning Plugin remains authoritative for whether the scope exists, whether
a subject is eligible for it, and every resource-local business rule. A target
Plugin owns the meaning of permission identifiers and performs final
authorization. Access Control returns only the RBAC factor: an Operation that
declares RBAC as required must fail closed on denial or unavailability, while an
allow is necessary but never sufficient to bypass membership, resource
ownership, state, or other target-owned checks. Authentication remains a
separate Auth Plugin concern.

The first portable Interfaces are a consumer-facing
`lenso.access-control@1` request Capability for checking one scoped permission
and an administrative `lenso.access-control-admin@1` request Capability for
managing roles, grants, and bindings. Organization replaces
`lenso.organization-access@1` with `lenso.organization-membership@1`, which
exposes only Organization-owned membership facts. Consumers receive these
Capabilities through resolved Ports; neither Plugin reads the other's tables
or uses a global policy registry.

Access Control authorizes its own administrative mutations. Bootstrap is
available only to exact App-admitted caller Instances; after bootstrap, scoped
role, grant, and binding changes require an authenticated actor with the
corresponding Access Control administration permission. Binding a caller to the
administrative Capability is necessary transport authority, not sufficient
business authorization.

The first Access Control Contract is deliberately allow-only RBAC. Missing
bindings deny by default. A subject may receive multiple roles within one scope
and its effective permissions are their union. Direct subject grants, role
inheritance, explicit deny, conditional attributes, and relationship traversal
are outside this Contract. Scope kinds are stable names and scope owners must
not reuse scope identifiers, so delayed binding cleanup cannot reactivate an
old subject against a different resource.

## Considered options

- Keeping roles and permissions inside Organization makes the first
  Organization owner transaction simple, but ties authorization to one scope
  type and prevents Project, Workspace, Document, and other Plugins from using
  the same independent policy owner.
- Letting Access Control own Organization membership avoids a second check but
  turns a business fact used by invitations, directories, seats, and lifecycle
  into an authorization implementation detail.
- A shared database or cross-Plugin table access makes a split appear atomic
  while creating two authorities over the same state. Lenso instead uses
  explicit Capability calls and honest partial-failure handling.

## Consequences

- The Organization schema no longer contains role definitions, permission
  arrays, or role bindings. Membership records remain Organization-owned and
  contain no Access Control role identifier. Organization introduces ownership
  as a first-class relationship and enforces that every active Organization has
  at least one owner; ownership no longer exists only as an Access Control role.
- Organization creation and initial access bootstrap are separate idempotent
  transitions. A product that promises them as one user-visible workflow must
  own orchestration, expose an honest pending or failed state, retry safely, and
  fail closed until both transitions succeed.
- Membership removal or Organization archival denies access even if cleanup of
  an Access Control binding is delayed. Target workflows must check current
  scope eligibility in addition to the role decision.
- The current `lenso.organization-access@1` Interface and Organization-owned
  permission storage are retired during the vNext migration rather than kept as
  a compatibility authorization path. The existing Organization Admin response
  exposes an Organization-owned role identifier that has no meaning after the
  split, so the replacement administration Interface is
  `lenso.organization-admin@2` rather than a silent change to
  `lenso.organization-admin@1`.
- The first implementation is PostgreSQL-backed RBAC. Conditional policies,
  explicit deny rules, relationship graphs, and policy languages require a new
  deliberate contract decision instead of silently changing RBAC semantics.
- Every role, grant, and binding mutation advances a monotonic policy revision
  in the same Access Control transaction. Optional Audit delivery uses an
  explicit Capability and an owner-local outbox; it is not a synchronous
  cross-Plugin transaction. Runtime invocation telemetry and target-owned
  authorization failures do not become Access Control private state.
