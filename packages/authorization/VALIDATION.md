# Authorization validation

## Initial implementation checks

Validated locally with Bun **1.4.2**, TypeScript **7.0.2**, Drizzle **0.45.3**
and PostgreSQL **18.6**. Dependencies were restored from local cache with
`bun install --offline --no-save --ignore-scripts`; `bun.lock` was not changed.

- Built `packages/lenso` and `packages/auth` before using their public exports.
- `bun run --cwd packages/authorization build`: declaration generation and split
  Bun build passed.
- `bun run --cwd packages/authorization typecheck`: passed.
- `bun run --cwd packages/authorization test`: **64 passed, 1 PostgreSQL test
  skipped by default, 0 failures** across 12 files.
- The PostgreSQL test was also run separately against a newly initialized,
  task-owned loopback PostgreSQL cluster and `authorization_fixture` database:
  **1 passed, 0 failures**. The cluster was stopped afterward. No existing database
  was touched.
- `oxlint packages/authorization --deny-warnings`, package-only `oxfmt --check`,
  and `git diff --check`: passed.
- `bun pm pack --ignore-scripts` produced a local artifact containing declarations,
  split JS chunks, migrations and documentation. In a scratch consumer with none
  of Auth/core/Drizzle/Manage installed, the packed root imported and performed
  actual RBAC allow/deny checks.
- All eight declared public JavaScript entry points loaded through package exports
  with their selected workspace peers installed. Nothing was published.
- Final independent source review covered core/attribute ordering, Auth provenance
  and snapshots, management grant/edit/delegation, RBAC/list handling, persistent
  graph validation/CAS and optional dependency boundaries. It found no additional
  source-backed blockers under the documented trust model. This review did not
  independently execute tests, generated declarations or packed consumers.

The PostgreSQL command used the test's exact opt-in variables
`AUTHORIZATION_TEST_PG_URL` (task-owned loopback URL) and
`AUTHORIZATION_TEST_PG_OWNED=1`, then ran
`bun test packages/authorization/test/pg.test.ts`. No production connection,
credential, notification, charge, deployment or publication was used.

## Behavior covered

- Full allow/deny/abstain conflict matrix in both orders; matching explicit deny
  precedence; default denial; exact scope/action/resource matching.
- AND narrowing versus explicit OR; missing attributes; direct relationships;
  anonymous public resources; explicit approved cross-org access and refusal.
- Credential presence, expiry and permission intersection before custom grants.
- Real existing Auth provenance checks for copied/JSON/foreign actors, wrong
  audience, source revocation and membership loss; source/facts exception safety.
- Auth resource/membership snapshots across controlled asynchronous races.
- Independent management actions, bounded grant scopes/resources/expiry,
  inherited and staged role-edit elevation, malformed/cyclic role graphs,
  recipient-aware authorization and JSON-principal refusal through real Auth.
- Current-store revocation and concurrent revision conflicts, including
  revocation while a management authorizer is paused.
- Lists checked before pagination, visible-only total, bounded fallback refusal
  and no partial result on evaluated failures.
- Timeout, abort, malformed policy outputs, resolver errors and late success;
  explanation gate with ordinal paths and safe denial messages.
- Real SQLite owner/version conditional update after a concurrent owner change.
- Actual SQLite migration/driver, JSON corruption, namespace isolation, immutable
  snapshots, unchanged/stale revision refusal and one conditional-write winner.
- Local **Miniflare D1** migration and real Drizzle D1 driver, `UPDATE RETURNING`,
  concurrent CAS and invalid graph rejection. This is an emulator check, not a
  Cloudflare deployment.
- Actual PostgreSQL migration/Bun SQL JSONB roundtrip, namespaces, immutable read,
  revocation document update, concurrent CAS and malformed graph rejection.
- Lenso exact instance dependencies, borrowed resource ownership and existing
  Config binding. Four minimal executable recipes reuse the Notes domain.

## Landing integration

The landing integration, explicitly authorized by the user, registers this
workspace and its dependencies in the shared `bun.lock`. The resulting
`bun install --frozen-lockfile` passed without further lockfile changes.
`scripts/ci-checks.sh` creates an additional task-owned `authorization_fixture`
database in its own temporary cluster and explicitly runs this package's
PostgreSQL test, rather than treating its default skip as backend validation.
Ambient Authorization test connection/ownership variables are cleared first.
The integration also records the public package change through Changesets;
no package versioning or registry publication is performed by this landing.

## Deliberate limits and unverified items

- The initial implementation did not run the whole-repository pipeline or a live
  Web/CLI/MCP/Manage deployment. No transport operations were auto-registered.
- Public Auth does not expose verified API-key scopes; application-owned verified
  ceiling readers are required. Anonymous Auth `enforce` is not available; an
  explicit public pure-core entry is documented instead.
- No external SSO, policy/relationship provider, deployed D1, replica-lag/failover,
  other PostgreSQL version or other platform certification was tested.
- No SQL compiler is provided. Lists use a complete bounded candidate set or
  must refuse pending an independently reviewed application constraint.
- No positive cross-request cache or invalidation protocol is provided. Revocation
  takes effect when a fresh authoritative read sees the committed revision;
  old decisions and in-flight snapshots remain old.
- Whole-graph CAS fences changes in the same role store. It does not atomically
  fence separate Auth/membership/credential/resource stores, time expiry, approval
  owners or external side effects. Business writes must recheck/fence their own
  mutable state.
- Delegation creates independent bindings, with no cascading revocation or
  permanent issuance-permission snapshot through later authorized role edits.
- Timers cannot preempt synchronous JS or forcibly settle providers. Async
  callbacks must cooperate with cancellation. A cancelled committed mutation
  can leave an uncertain caller outcome; it is not automatically retried.
- Optional observation is not a durable audit store or an atomic audit/write
  transaction. No strict global consistency or exactly-once guarantee is claimed.
