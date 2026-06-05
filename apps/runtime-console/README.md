# Runtime Console

Frontend prototype for the Lenso Runtime Console.

The console runs with seeded data by default, and switches core runtime views to the
local backend when `VITE_RUNTIME_CONSOLE_MODE=api` and `VITE_API_BASE_URL` are set.

Local API calls use the development service token:

```text
Authorization: Bearer dev-service:admin
```

## Run

```bash
cd apps/runtime-console
pnpm install
pnpm dev
```

Open:

```text
http://localhost:5174
```

## Backend Wiring

Start the backend and worker from the repo root:

```bash
just db-up
just migrate
just api
just worker
```

Run the console against the local API:

```bash
VITE_RUNTIME_CONSOLE_MODE=api VITE_API_BASE_URL=http://localhost:3000 pnpm dev
```

Local API calls use the development service token:

```text
Authorization: Bearer dev-service:admin
```

## Remote Module API QA

From the repo root, start a full remote-module Runtime Console demo:

```bash
just console-api-demo
```

Then seed and verify the remote story path:

```bash
just console-api-qa
```

Useful focused commands:

```bash
just console-api-fixture
just console-api-smoke
```

The QA fixture creates a remote proxy call with
`correlation_id = corr_console_api_fixture`, then verifies the Remote Calls page
data, Runtime Story remote node/timeline shape, Technical Operations, payloads,
and logs.

If Postgres is already running and migrated:

```bash
SKIP_DB_SETUP=1 just console-api-demo
```

If default ports are busy:

```bash
REMOTE_MODULE_ADDR=127.0.0.1:4101 HTTP_PORT=3001 VITE_API_BASE_URL=http://localhost:3001 CONSOLE_PORT=5176 just console-api-demo
```

## Architecture

- `src/app`: router and root providers.
- `src/components/ui`: small Tailwind-composed primitives.
- `src/components/runtime`: Runtime Console shell, search, command palette, drawer, timeline nodes.
- `src/data`: seeded mock runtime data.
- `src/hooks`: keyboard and runtime query hooks with API/mock switching.
- `src/lib`: formatting, query client, and ky HTTP client foundation.
- `src/pages`: route-level screens.

## Checks

The console uses Ultracite with the Oxlint/Oxfmt provider:

- `oxlint.config.ts` extends `ultracite/oxlint/core`, `ultracite/oxlint/react`, and `ultracite/oxlint/tanstack`.
- `oxfmt.config.ts` extends `ultracite/oxfmt`.
- No ESLint, Prettier, or Biome stack is configured.

```bash
pnpm format
pnpm format:check
pnpm lint
pnpm typecheck
pnpm build
pnpm check
```
