# API Key implementation checks

## Actual results

- Bun 1.4.2; PostgreSQL 18.6 (Homebrew), private local clusters.
- `bun run --cwd packages/api-keys build`: passed, public JS and declarations built.
- `bun run --cwd packages/api-keys typecheck`: passed.
- `LENSO_REQUIRE_POSTGRES=1 bun run --cwd packages/api-keys test`:
  **26 passed, 0 failed, no skips**, 368 assertions across 6 test files.
- `node_modules/.bin/oxlint packages/api-keys --deny-warnings`: no warnings/errors.
- `node_modules/.bin/oxfmt --check packages/api-keys`: passed.
- `git diff --exit-code -- bun.lock package.json packages/auth packages/tasks packages/manage`:
  passed; shared packages, root manifest and root lockfile unchanged.
- `git diff --check`: passed.

Core, Engine, Web, Auth and Manage builds were run first to make current public
exports available. Local dependency setup used `bun install --no-save
--ignore-scripts` after adding this workspace package, without saving the lockfile.

## Evidence boundaries

Core unit tests use a deterministic test-only Store. Backend suites separately
exercise real Bun SQLite, real private PostgreSQL with Bun SQL/Drizzle, and
actual local workerd D1 through Miniflare 5.20261006.0-alpha.
The complete ordinary credential service is tested against each native Store:
issue/replay, digest-only persistence, wrong secret, exact scope ceiling,
permission withdrawal, competing rotation, overlap acceptance and boundary,
zero-overlap invalidation, expiry and successor revocation.
PostgreSQL tests additionally queue distinct connections behind row locks and
verify expiry after waiting and both rotation/revocation orderings.

Auth/Manage tests use public exports to check same-realm explicit source routing,
source namespace collisions, long/escaped identity references, current
membership withdrawal, revoked proof, cloned actor, wrong audience, wrong Auth
instance, JSON caller injection and cross-tenant management.
Config uses the exact public binding. Package testing copies built output to a
disposable directory and imports the root without optional peers installed.
Only owned test resources and test credentials were used; owned resources are
cleaned up. No production database, provider or authorization was touched.

## Unverified and integration-owner follow-ups

- No full-workspace regression suite or release/publish/deployment workflow run.
- No production Cloudflare replication check, full credential core execution
  inside workerd, other PG driver verification, or blanket platform claim.
- No strict cross-resource consistency, exactly-once issuance, automatic audit
  persistence, live-request cancellation or business-write transaction guarantee.
- Current Auth source verification has no operation audience and no non-session
  post-policy credential finalization hook. Applications must install matching
  fixed scope profiles and retain `keys.use` at the service boundary after
  asynchronous `enforce`. Tests cover the resulting scope/expiry mitigation.
  Public audience-aware source/finalization APIs, if wanted, belong to the
  unified Auth integration owner; no private-state workaround is implemented.
- The integration owner must update the single workspace Bun lockfile before
  frozen-lockfile CI includes this new package.

## Primary references reviewed

- [Node crypto](https://nodejs.org/api/crypto.html): CSPRNG, SHA-256 and fixed-size timing-safe comparison.
- [D1 sessions](https://developers.cloudflare.com/d1/worker-api/d1-database/#withsession):
  fresh `first-primary` operation sessions, not stale replica/cache reads.
- [PostgreSQL row locks](https://www.postgresql.org/docs/current/explicit-locking.html#LOCKING-ROWS):
  explicit row ownership before rotation's database-time predicate.
