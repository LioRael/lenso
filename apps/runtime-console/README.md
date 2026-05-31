# Runtime Console

Frontend prototype for the Lenso Runtime Console.

The current console intentionally runs in mock mode only. The API client foundation is present for
the next backend wiring slice, but screens still read from mock TanStack Query hooks.

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

## Architecture

- `src/app`: router and root providers.
- `src/components/ui`: small Tailwind-composed primitives.
- `src/components/runtime`: Runtime Console shell, search, command palette, drawer, timeline nodes.
- `src/data`: seeded mock runtime data.
- `src/hooks`: keyboard and mock runtime query hooks.
- `src/lib`: formatting, query client, and future ky HTTP client foundation.
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
