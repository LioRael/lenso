# Module Registry V0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add local catalog-driven remote module discovery and installation to the existing Lenso module CLI.

**Architecture:** The catalog reader and registry commands live inside `apps/runtime-console/packages/console-package-cli/src/index.mjs` next to `module add` and `module doctor`. `registry install` delegates to the existing `addRemoteModule` implementation so env updates, install-plan writes, and doctor checks stay centralized.

**Tech Stack:** Node.js ESM, Commander, Vitest, existing Runtime Console CLI package.

---

### Task 1: Registry Catalog Tests

**Files:**
- Modify: `apps/runtime-console/packages/console-package-cli/src/index.test.mjs`

- [ ] **Step 1: Add tests for `registry list`, `registry inspect`, and `registry install`**

Add fixture catalog tests that use the existing temp repo and manifest server helpers. Verify list output includes module name, version, manifest reference, capabilities, and console package. Verify inspect fetches the manifest and rejects catalog/manifest mismatches. Verify install writes the same `.env` and install plan shape as `module add`.

- [ ] **Step 2: Run focused test and verify failure**

Run:

```sh
pnpm --dir apps/runtime-console exec vitest run packages/console-package-cli/src/index.test.mjs
```

Expected: registry command tests fail because the command does not exist yet.

### Task 2: Registry CLI Implementation

**Files:**
- Modify: `apps/runtime-console/packages/console-package-cli/src/index.mjs`

- [ ] **Step 1: Add catalog helpers**

Implement `readModuleRegistry`, `normalizeRegistryEntry`, `findRegistryModule`,
and output formatting helpers. Reject non-object catalogs, non-array modules,
missing required fields, duplicate module names, and non-remote sources.

- [ ] **Step 2: Add command handlers**

Implement:

```sh
lenso module registry list
lenso module registry inspect <moduleName>
lenso module registry install <moduleName>
```

`install` should call `addRemoteModule({ manifestReference, options })` with
the catalog entry's `manifestReference` and `baseUrl`.

- [ ] **Step 3: Wire Commander commands**

Add a `registry` subcommand under `module`, with shared `--registry-file`,
`--repo-root`, `--env-file`, `--install-plan-file`, and `--base-url` behavior
where relevant.

- [ ] **Step 4: Run focused test and verify pass**

Run:

```sh
pnpm --dir apps/runtime-console exec vitest run packages/console-package-cli/src/index.test.mjs
```

Expected: all console package CLI tests pass.

### Task 3: Documentation

**Files:**
- Modify: `docs/architecture/third-party-modules.md`
- Modify: `apps/runtime-console/docs/remote-module-install-flow.md`
- Modify: `apps/runtime-console/packages/console-package-cli/src/index.test.mjs`

- [ ] **Step 1: Document Registry v0**

Document the catalog format and command sequence. Keep marketplace trust and
automatic npm installation in deferred support.

- [ ] **Step 2: Extend docs regression tests**

Assert that the architecture and install-flow docs mention `module registry`,
`registry list`, `registry inspect`, and `registry install`.

- [ ] **Step 3: Run final Runtime Console gate**

Run:

```sh
just console-check
```

Expected: format, lint, tests, typecheck, and build pass.
