# Team Desk

A persistent team work tracker implemented independently with the TS and Rust SDKs. Four separately authored modules contribute ordinary Actions and Console operations: members, projects, work items and notifications. A separate [Console host](../../typescript/examples/team-desk-console/README.md) provides the browser read/write workflow for both backends.

## Run

From the repository root, build the selected implementation:

```sh
npm ci
npm run build
cargo build --manifest-path rust/Cargo.toml -p lenso-team-desk
```

Load the deliberately public **local demo** identities, then start either server. Both bind to loopback. Use different database paths if you want independent applications; the examples deliberately share a schema and can also reopen the same database.

```sh
export LENSO_IDENTITIES="$(cat examples/team-desk/identities.example.json)"
export LENSO_DB=.local/team-desk.sqlite
PORT=3000 npm run start --workspace @lenso/example-team-desk
# Or, in place of the TS server:
PORT=3001 rust/target/debug/lenso-team-desk
```

Tokens come from trusted host configuration. Request bodies cannot choose a team or role; the native context may include unrelated grants in any order. Initial membership is inserted only when absent; startup never overwrites later role changes. The bundled tokens are for local examples; authentication-provider integration is application work. The global notification worker can be driven explicitly with `POST /worker/drain` by a team admin; it processes at most 100 jobs per call, including other teams' jobs, without exposing their data. It is an operational demo endpoint, not a tenant-specific business operation. No background worker starts implicitly.

## Browser Console

After building both language examples and the browser host, run `npm run team-console` from the root and open `http://127.0.0.1:5173`. The launcher uses separate persistent databases and public local demo identities. The same compiled pages work with either backend. See the [host guide](../../typescript/examples/team-desk-console/README.md) for roles, validation, conflict recovery and browser tests.

## Console client workflow

With the server running on port 3000, execute this from a Node ESM session in the repository root (`node --input-type=module` or a local `.mjs` file):

```js
import { ConsoleClient } from '@lenso/console';
const console = new ConsoleClient('http://127.0.0.1:3000/console', () => 'alice-local-secret-token');
const manifest = await console.manifest();
const projects = manifest.pages.find(page => page.id === 'projects');
const items = manifest.pages.find(page => page.id === 'items');
await console.call(projects.actions.find(id => id === 'projects.create'), { id: 'launch', name: 'SDK launch' });
let item = await console.call(items.actions.find(id => id === 'items.create'), {
  id: 'release', project: 'launch', title: 'Ship the SDK', assignee: 'bob',
});
item = await console.call('items.assign', { id: item.id, version: item.version, assignee: 'alice' });
item = await console.call('items.transition', { id: item.id, version: item.version, status: 'doing' });
item = await console.call('items.transition', { id: item.id, version: item.version, status: 'done' });
await fetch('http://127.0.0.1:3000/worker/drain', {
  method: 'POST', headers: { authorization: 'Bearer alice-local-secret-token', 'content-type': 'application/json' }, body: 'null',
});
const inbox = await console.call('notifications.list');
```

Results from `ConsoleClient.call` are unknown until validated by the consuming page. The JS walkthrough is exploratory; the TS implementation defines the domain parsers in its modules. Repeating a create with the same id returns `conflict`; this workflow does not silently erase existing data.

## Business contract

- Admins change other members' roles and archive projects. Admins and editors create projects and create, assign and advance work items. Viewers can read their own team's data.
- Roles are read from storage on every operation, so a downgrade applies to existing tokens immediately. Self-role changes are rejected to avoid accidental loss of the only administrator.
- Every lookup uses the authenticated team. Foreign project/item ids return `not_found`; foreign or read-only assignees are rejected. Notifications are visible only to their recipient within the authenticated team.
- Work items advance `open -> doing -> done`. Assignment and transitions require the current version. Concurrent stale writes return `conflict`; the client must refresh rather than retry blindly. Archived projects reject item mutations.
- Identifiers are 1–64 ASCII letters, digits, underscores or hyphens. Request versions range from 1 through 2,147,483,646; the incremented stored/result revision may reach 2,147,483,647. Project names and item titles use the same trimming set in both implementations, including copied BOM characters, and are limited to 120/200 UTF-16 units. Shared text fixtures cover the distinction from Rust native trim behavior.
- Item creation and each subsequent mutation publish `items.changed` inside the same native SQLite transaction. The composition root connects that event to the notification module. The producer names neither the subscriber nor its task.
- Delivery can repeat. The notification task deduplicates by job id with a database constraint. Records contain operation ids, subjects and outcomes, without request bodies or tokens; records are observability, not transactional auditing.

## Verification

Build both implementations, then run `node --test integration/team-desk.test.mjs` or the full `npm run test:integration`. The tests exercise actual HTTP processes and the shipped Console client, including writes, state transitions, role changes, isolation, conflicting writes, persisted tasks, restart and TS/Rust interchange. A SQLite trigger injects publication failure to prove the domain write rolls back. Native-call tests and independent module installation are also covered.

The shared SQL file describes only this application's persisted data contract. Neither SDK requires shared application schemas or cross-language interoperability. Production identity, pagination, database migrations beyond initial creation, retention, production deployment, the full Official Console product and performance validation remain outside this reference application.
