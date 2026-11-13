# Lenso SDK

Native application SDKs for TypeScript and Rust, developed together and built independently.

Source: [LioRael/lenso](https://github.com/LioRael/lenso). The previous Bun-first framework is preserved in [lenso-previous](https://github.com/LioRael/lenso-previous).

This repository implements the first SDK baseline described in the [Lenso product design](https://chatgpt.com/space/page_8d106197e7e08191b3a2ef80dee240e7). The SDKs do not require a CLI process, a Console process, a Rust host for TypeScript, or RPC for same-language calls.

## Packages

| Package | Purpose |
| --- | --- |
| `@lenso/app` / `lenso-app` | Typed action execution, access checks, native resource injection, module descriptions, local events, task definitions and Console extension descriptions |
| `@lenso/sqlite` / `lenso-sqlite` | Durable tasks and event fanout using SQLite, with retries, delays, leases, cancellation, persisted results and native transaction integration |
| `@lenso/console` | Headless TypeScript client and compiled custom-page registration for authorized Console extensions |

The SDKs use MIT. See [publishing](docs/publishing.md) for registry delivery and Trusted Publishing. `@lenso/console` 3.x provides this headless client; 2.x is the previous Console host with a different API.

Install the core and the optional packages your application needs:

```sh
npm install @lenso/app@^0.1.1
npm install @lenso/sqlite@^0.1.1 @lenso/console@^3.0.0
```

```sh
cargo add lenso-app@0.1.1
cargo add lenso-sqlite@0.1.1
```

The SQLite examples use Node.js 26 or Rust 1.99. The SQLite crate declares Rust 1.89 as its dependency-driven minimum; the core declares 1.85. Minimum-version builds are not yet verified.

## Run the private file library

The examples provide actual registration, password verification, expiring bearer sessions, owner-protected file upload/download and SQLite persistence. File metadata and bytes live in the same database. Both languages implement the application independently.

TypeScript, using Node.js 26 and npm:

```sh
npm ci
npm run build
npm run private-files
```

Rust, using the installed Rust toolchain:

```sh
cargo run --manifest-path rust/Cargo.toml -p lenso-private-files
```

The TypeScript server listens on `127.0.0.1:3000`; the Rust server uses `127.0.0.1:3001`. Each writes `.local/private-files.sqlite` relative to its process working directory. Set `PORT` and `LENSO_DB` to override these. Use different databases for separate applications. The two examples deliberately share a database format for interoperability tests; this is not a permanent storage compatibility guarantee.

```sh
curl http://127.0.0.1:3000/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"name":"alice","password":"a-long-local-password"}'

curl http://127.0.0.1:3000/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"name":"alice","password":"a-long-local-password"}'
```

Use the returned token as `Authorization: Bearer TOKEN`. `POST /files` accepts `{"name":"hello.txt","content":"aGVsbG8="}`; content is base64 on HTTP and native bytes inside the application. `GET /files` lists the caller's files; `GET /files/ID` reads one. The maximum decoded file size is 1 MiB.

## Console extension integration

The example exposes `GET /console/manifest` and read-only `POST /console/actions/console.users`, `console.files`, and `console.requests`. Lists are bounded to 100 rows. Every management action uses the same executor and requires `console:read`; authentication alone does not grant it.

To grant local read-only management access, set `LENSO_CONSOLE_SUBJECT` to a registered user's returned **id** before starting the server. This is trusted deployment configuration, not a request field. It is not a production role-management system.

```ts
import { ConsoleClient } from "@lenso/console";

const console = new ConsoleClient("http://127.0.0.1:3000/console", () => token);
const manifest = await console.manifest();
const files = await console.call("console.files");
```

A separate [Team Desk browser host](typescript/examples/team-desk-console/README.md) is included as a reference consumer. Backend `ConsoleExtension` bindings generate their page descriptions and dispatch list/detail/custom operations through the same application Actions. The client SDK loads descriptions, calls authorized operations and registers compiled custom page factories through `ConsolePages`. Custom views can use any frontend framework. Closing a Console client has no effect on the backend.

## Invitations and registration notifications

The workflow examples combine all seven SDK areas. They create a user and an invitation through authorized native actions, atomically enqueue their notification jobs with the domain writes, run the durable worker, publish a local registration event and describe an authorized mailbox Console page.

```sh
npm run workflows
cargo run --manifest-path rust/Cargo.toml -p lenso-workflows
```

Mail is delivered to a **local SQLite mailbox resource**, not an external email service. No email is sent. Both examples use `.local/workflows.sqlite` by default. Re-running either example preserves two domain records and two delivered messages without duplicate mail; the stable job id is also the mailbox's idempotency key. Replacing this resource with an external mail provider requires that provider's own delivery and idempotency policy.

## Multi-module application with Console writes

[Team Desk](examples/team-desk/README.md) provides independent members, projects, work-items and notifications modules in both languages. It exercises real Console create/assign/transition/archive operations, stored roles, tenant isolation, optimistic versions and transactional notification delivery. The guide includes server commands and a Console-client write workflow. The independent [browser host](typescript/examples/team-desk-console/README.md) supports both backends with the same compiled pages, including form validation, permissions and explicit conflict recovery. After building both implementations, run `npm run team-console` and open `http://127.0.0.1:5173`.

## Development and verification

```sh
npm ci
npm run typecheck
npm run lint
npm test
npm run test:package

cargo fmt --manifest-path rust/Cargo.toml --all --check
cargo check --manifest-path rust/Cargo.toml --workspace --all-targets --all-features
cargo test --manifest-path rust/Cargo.toml --workspace --all-features
cargo test --manifest-path rust/Cargo.toml -p lenso-app --no-default-features
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets --all-features -- -D warnings
cargo doc --manifest-path rust/Cargo.toml --workspace --all-features --no-deps

cargo build --manifest-path rust/Cargo.toml --workspace
npm run test:integration
```

TypeScript tests do not invoke Cargo. Rust tests do not invoke Node. Cross-language integration checks require all built example binaries. Root `typecheck` builds dependency declarations as part of strict checking, so it also works without pre-existing `dist` files. Package verification installs packed archives and the installed Zod dependency into an offline temporary consumer and checks emitted declarations and runtime exports. Each SDK's `prepack` hook builds its distribution before packaging.

Shared action and task fixtures live in `conformance/fixtures/`. Both language suites execute them. Integration tests start actual HTTP processes, check identity isolation, restart persistence, management authorization and TS-to-Rust database interoperability. See [semantics](docs/semantics.md) for the precise guarantees, [verification](docs/verification.md) for executed checks and their limits, and [implementation status](docs/status.md) for scope and remaining product work.
