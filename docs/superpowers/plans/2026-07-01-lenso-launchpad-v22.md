# Lenso Launchpad V22 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the minimal Launchpad path: create a support-desk app, inspect dev status, emit agent context, and show Launchpad state in Console.

**Architecture:** Reuse existing host/service scaffolds and existing admin-data JSON-reading patterns. Store Launchpad state in generated `.lenso/launchpad.json`; do not add a daemon, workflow engine, Kubernetes dependency, or AI provider.

**Tech Stack:** Rust CLI, Rust admin-data endpoint, React Runtime Console, Markdown docs, JSON fixtures.

---

### Task 1: CLI Launchpad Commands

**Files:**
- Modify: `lenso-cli/src/main.rs`
- Create: `lenso-cli/src/launchpad.rs`

- [ ] **Step 1: Write failing CLI tests**

Add parser tests for:

```rust
lenso app create support-desk --blueprint support-desk
lenso dev status
lenso agent context
```

Run: `cargo test --locked launchpad`
Expected: compile failure because the commands do not exist.

- [ ] **Step 2: Add command structs**

Add top-level `App`, `Dev`, and `Agent` subcommands. Keep them thin and dispatch to `launchpad.rs`.

- [ ] **Step 3: Implement generated state**

Create `LaunchpadState` with project name, blueprint, services, modules, commands, and checklist. Write it to `.lenso/launchpad.json`.

- [ ] **Step 4: Implement `app create`**

Reuse `host::init` and `service::create_service`. Generate the support-desk host, TS service, Rust service, `lenso.system.json`, and Launchpad state.

- [ ] **Step 5: Implement `dev status/stop`**

`status` reads `.lenso/launchpad.json` and prints configured services/modules/next command. `stop` prints that `dev up` is foreground and stops with Ctrl-C.

- [ ] **Step 6: Implement `agent context/task`**

Read `lenso.system.json`, `lenso.workspace.json`, and `.lenso/launchpad.json`; emit Markdown. `task` appends the requested task text.

- [ ] **Step 7: Verify CLI**

Run:

```sh
cargo test --locked launchpad
cargo test --locked app_command dev_command agent_command
```

### Task 2: Host Launchpad Endpoint

**Files:**
- Modify: `lenso/crates/platform-admin-data/src/dto.rs`
- Modify: `lenso/crates/platform-admin-data/src/handlers.rs`
- Modify: `lenso/crates/platform-admin-data/src/lib.rs`

- [ ] **Step 1: Write failing endpoint test**

Add a test that writes `.lenso/launchpad.json`, calls the response helper, and expects `ready`, project name, blueprint, counts, and next command.

Run: `cargo test -p lenso-platform-admin-data launchpad`
Expected: compile failure because the DTO/helper do not exist.

- [ ] **Step 2: Add DTOs and response helper**

Follow the existing service-system helpers. Missing file returns `empty` and `lenso app create support-desk --blueprint support-desk`.

- [ ] **Step 3: Add route**

Register `GET /admin/data/launchpad`.

- [ ] **Step 4: Verify backend**

Run:

```sh
cargo test -p lenso-platform-admin-data
just generate-contracts
just generated-check
just arch-check
```

### Task 3: Runtime Console Launchpad Page

**Files:**
- Modify: `lenso-runtime-console/src/data/available-modules.ts`
- Modify: `lenso-runtime-console/src/pages/available-modules-model.ts`
- Create: `lenso-runtime-console/src/pages/launchpad-page.tsx`
- Create: `lenso-runtime-console/src/pages/launchpad-model.ts`
- Create: `lenso-runtime-console/src/pages/launchpad-model.test.ts`
- Modify: `lenso-runtime-console/src/app/router.tsx`
- Modify: `lenso-runtime-console/src/components/runtime/runtime-console-shell.tsx`

- [ ] **Step 1: Write failing model/fetch tests**

Test that Launchpad response summarizes project, blueprint, counts, checklist, and next command.

Run: `pnpm test -- launchpad-model.test.ts available-modules.test.ts`
Expected: failure because the types/functions do not exist.

- [ ] **Step 2: Add fetch boundary**

Add `fetchLaunchpad`, query key, sample response, and response types.

- [ ] **Step 3: Add page and route**

Add `/launchpad` route and a compact page that shows the Launchpad summary and checklist. Set root redirect to `/launchpad`.

- [ ] **Step 4: Verify Console**

Run:

```sh
pnpm test -- launchpad-model.test.ts available-modules.test.ts
pnpm check
```

### Task 4: Examples And Site Proof

**Files:**
- Modify: `lenso-examples/README.md`
- Create: `lenso-examples/fixtures/launchpad/support-desk/launchpad.json`
- Create: `lenso-examples/fixtures/launchpad/support-desk/agent-context.md`
- Modify: `lenso-site/content/docs/(host)/quickstart.mdx`
- Modify: `lenso-site/content/docs/(host)/cli-reference.mdx`

- [ ] **Step 1: Generate fixtures**

Use `lenso app create` in a temp directory, copy `.lenso/launchpad.json`, and capture `lenso agent context`.

- [ ] **Step 2: Document quickstart**

Put Launchpad commands before deeper service-system commands.

- [ ] **Step 3: Verify examples and site**

Run:

```sh
git diff --check
pnpm types:check
pnpm lint
```

### Task 5: Commit

Commit each touched repo:

```sh
git commit -m "feat: add launchpad app workflow"
git commit -m "feat: expose launchpad admin data"
git commit -m "feat: add launchpad console page"
git commit -m "docs: add launchpad fixtures"
git commit -m "docs: document launchpad quickstart"
```

## Self-Review

- Spec coverage: CLI create/status/context, Host endpoint, Console page, examples, and site are covered.
- Scope check: background process management, Kubernetes, marketplace, and AI provider integration are explicitly out.
- Placeholder scan: no TBD/TODO items are required for implementation.
