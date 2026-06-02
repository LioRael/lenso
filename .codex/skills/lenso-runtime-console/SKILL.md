---
name: lenso-runtime-console
description: Use when changing Lenso's Vite React Runtime Console under apps/runtime-console, including operator UI screens, runtime visualization components, TanStack Query or Router code, Tailwind styling, Base UI primitives, tests, linting, typechecking, or console build behavior.
---

# Lenso Runtime Console

## Purpose

Use this skill for changes under `apps/runtime-console`. Preserve the existing operator-console style: dense, scannable, workflow-focused, and built from existing UI primitives when possible.

## Stack

- Vite, React 19, TypeScript, Tailwind CSS v4
- TanStack Query and TanStack Router
- Base UI primitives
- Lucide icons
- Ultracite, Oxfmt, Oxlint
- Vitest

## Project Conventions

- Prefer existing primitives in `apps/runtime-console/src/components/ui`.
- Keep screen logic decomposed into model/test files when the repo already follows that pattern.
- Use `pnpm --dir apps/runtime-console ...` for direct package scripts.
- Do not add ESLint, Prettier, or Biome.
- Keep operational screens information-dense and keyboard-friendly.
- Preserve current visual language unless the user asks for a redesign.

## Workflow

1. Inspect the local console patterns near the target page or component.
2. Check for existing model helpers and tests before adding component state.
3. Prefer derived data in plain model functions when behavior can be unit tested without rendering.
4. Use existing hooks under `src/hooks` for API/query integration.
5. Update mock data in `src/data/mock-runtime.ts` only when the UI path needs representative states.
6. Add or update Vitest coverage for behavior changes.

## Commands

Run narrow checks while iterating:

```sh
pnpm --dir apps/runtime-console run test
pnpm --dir apps/runtime-console run typecheck
pnpm --dir apps/runtime-console run build
```

For substantial console changes, run:

```sh
just console-check
```

Formatting commands:

```sh
pnpm --dir apps/runtime-console run format
pnpm --dir apps/runtime-console run format:check
```

## API Integration

For local development:

```sh
just console
just console-api
```

Use `just console-api` when validating against `http://localhost:3000`. If API contract or generated SDK output changes are involved, also use `$lenso-contracts-sdk`.
