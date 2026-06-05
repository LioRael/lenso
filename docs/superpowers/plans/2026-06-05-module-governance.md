# Module Governance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first module governance slice: backend-derived capability reference checks plus a module activation/governance summary visible in the Runtime Console.

**Architecture:** Keep governance rules in Rust. `platform-module` owns capability reference derivation and manifest lint rules; `platform-admin-data` exposes a compact `governance` DTO on `/admin/data/modules`; the Runtime Console renders the DTO without reimplementing backend rules.

**Tech Stack:** Rust, utoipa/OpenAPI, Axum admin-data handlers, Vite/React Runtime Console, Vitest, generated TypeScript SDK.

---

### Task 1: Strengthen Manifest Capability Checks

**Files:**
- Modify: `crates/platform-module/src/manifest.rs`
- Test: `crates/platform-module/src/manifest.rs`

- [ ] **Step 1: Write failing tests**

Add tests that prove a module warns when admin read/action and HTTP route capabilities are referenced but not declared, while declared references stay clean.

- [ ] **Step 2: Run red test**

Run: `cargo test --locked -p platform-module manifest_lint_warns_for_undeclared_capability_references`
Expected: FAIL because no undeclared capability reference lint exists yet.

- [ ] **Step 3: Implement capability reference linting**

Collect referenced capabilities from `AdminSurface::Schema`, `AdminSurface::DeclarativeCustom`, `ModuleHttpRoute`, and `LifecycleStartupCheckKind::CapabilityDeclared`. Emit one warning per capability reference that is not present in `ModuleManifest.capabilities`.

- [ ] **Step 4: Run green test**

Run: `cargo test --locked -p platform-module manifest_lint_warns_for_undeclared_capability_references`
Expected: PASS.

### Task 2: Expose Module Governance DTO

**Files:**
- Modify: `crates/platform-admin-data/src/dto.rs`
- Modify: `crates/platform-admin-data/src/handlers.rs`
- Test: `crates/platform-admin-data/src/handlers.rs`

- [ ] **Step 1: Write failing tests**

Add a test proving `/admin/data/modules` metadata DTO includes `governance.activation_state`, `governance.capability_summary`, and capability warnings derived from backend lints.

- [ ] **Step 2: Run red test**

Run: `cargo test --locked -p platform-admin-data metadata_response_includes_module_governance`
Expected: FAIL because the DTO has no `governance` field yet.

- [ ] **Step 3: Implement DTO and derivation**

Add `AdminModuleGovernanceDto`, `AdminModuleActivationState`, `AdminCapabilitySummaryDto`, and `AdminCapabilityIssueDto`. Derive activation as `blocked` for load errors or error lints, `needs_attention` for warning lints, and `active` otherwise. Count declared/referenced/missing/unused capabilities from manifest data and lints.

- [ ] **Step 4: Run green test**

Run: `cargo test --locked -p platform-admin-data metadata_response_includes_module_governance`
Expected: PASS.

### Task 3: Render Governance In Modules Page

**Files:**
- Modify: `apps/runtime-console/src/pages/data-render-model.ts`
- Modify: `apps/runtime-console/src/pages/data-render-model.test.ts`
- Modify: `apps/runtime-console/src/pages/modules-page.tsx`

- [ ] **Step 1: Write failing model tests**

Add tests for `moduleGovernanceRows`, summary search text, and activation labels using the new `governance` DTO.

- [ ] **Step 2: Run red test**

Run: `pnpm --dir apps/runtime-console run test data-render-model.test.ts`
Expected: FAIL because the model helpers and types do not exist yet.

- [ ] **Step 3: Implement model helpers and UI rendering**

Add TypeScript types matching the Rust DTO, show activation state in the Modules list/detail header, and render a compact Governance section with activation, declared/referenced/missing/unused capability counts.

- [ ] **Step 4: Run green test**

Run: `pnpm --dir apps/runtime-console run test data-render-model.test.ts`
Expected: PASS.

### Task 4: Regenerate Contracts And Verify

**Files:**
- Generated: `contracts/openapi/app-api.v1.yaml`
- Generated: `packages/ts-sdk/src/generated/*`

- [ ] **Step 1: Regenerate**

Run: `just generate`
Expected: OpenAPI and TS SDK update to include the new governance DTO.

- [ ] **Step 2: Focused checks**

Run:
`cargo test --locked -p platform-module`
`cargo test --locked -p platform-admin-data`
`pnpm --dir apps/runtime-console run test data-render-model.test.ts`

- [ ] **Step 3: Repository checks**

Run:
`just generated-check`
`just sdk-check`
`just console-check`
`just arch-check`

- [ ] **Step 4: Review diff**

Run: `git diff --stat` and inspect changed files for unrelated edits. Leave `.codex/config.toml` untracked.
