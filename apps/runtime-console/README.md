# Runtime Console

Frontend prototype for the Lenso Runtime Console.

The console currently runs in mock mode with seeded data. A `ky` HTTP client foundation is present
for the next backend wiring slice, but screens intentionally stay mock-backed.

Future local API calls will use the development service token:

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

## Backend Wiring Later

Start the backend and worker from the repo root:

```bash
just db-up
just migrate
just api
just worker
```

API mode is reserved for the next slice:

```bash
VITE_RUNTIME_CONSOLE_MODE=api VITE_API_BASE_URL=http://localhost:3000 pnpm dev
```

Local API calls use the development service token:

```text
Authorization: Bearer dev-service:admin
```

## Architecture

- `src/app`: router and root providers.
- `src/components/ui`: small Tailwind-composed primitives.
- `src/components/runtime`: Runtime Console shell, search, command palette, drawer, timeline nodes.
- `src/data`: seeded mock runtime data.
- `src/hooks`: keyboard and runtime query hooks with mock defaults.
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
