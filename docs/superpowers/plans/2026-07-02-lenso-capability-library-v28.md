# Lenso Capability Library V28 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a local Capability Library so packs can be registered, checked, fit-tested, and composed by name.

**Architecture:** Keep the library file-backed in `.lenso/lenso.capability-library.json`. Reuse existing V27 pack parsing and App Change Plan logic; add only name resolution and a fit summary. Runtime Console remains read-only.

**Tech Stack:** Rust CLI, serde JSON, Rust admin-data DTOs, OpenAPI YAML, React Runtime Console, JSON examples, MDX docs, Lenso skills.

---

## Scope Check

This plan intentionally skips remote registries, signing, trust policy,
dependency solving, automatic installation, and browser-side writes.

## Files

- `/Users/leosouthey/Projects/framework/lenso-cli-v28/src/capability.rs`: add file-backed library model and commands.
- `/Users/leosouthey/Projects/framework/lenso-cli-v28/src/main.rs`: add `capability library` and `capability fit` parser/dispatch.
- `/Users/leosouthey/Projects/framework/lenso-cli-v28/src/launchpad.rs`: resolve `--pack` by path or library name and emit `packFit`.
- `/Users/leosouthey/Projects/framework/lenso-v28/crates/platform-admin-data/src/dto.rs`: add pack fit DTO.
- `/Users/leosouthey/Projects/framework/lenso-v28/crates/platform-admin-data/src/handlers.rs`: parse `packFit`.
- `/Users/leosouthey/Projects/framework/lenso-v28/contracts/openapi/app-api.v1.yaml`: regenerate.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console-v28/src/pages/available-modules-model.ts`: add pack fit type.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console-v28/src/pages/launchpad-model.ts`: summarize pack fit.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console-v28/src/pages/launchpad-page.tsx`: show pack fit.
- `/Users/leosouthey/Projects/framework/lenso-examples-v28/fixtures/capabilities/support-sla-pack/`: add library fixture files.
- `/Users/leosouthey/Projects/framework/lenso-examples-v28/scripts/check-launchpad-fixtures.mjs`: validate library fixture.
- `/Users/leosouthey/Projects/framework/lenso-site-v28/content/docs/(host)/product-blueprints.mdx`: document library flow.
- `/Users/leosouthey/Projects/framework/lenso-site-v28/content/docs/(host)/cli-reference.mdx`: add command rows.
- `/Users/leosouthey/Projects/framework/lenso-v28/skills/*.md`: teach agents the library flow.

## Tasks

### Task 1: CLI Library

- [ ] Add parser tests for `capability library add/list/check` and `capability fit`.
- [ ] Add `CapabilityCommand::Library` and `CapabilityCommand::Fit`.
- [ ] Add `CapabilityLibrary`, `CapabilityLibraryEntry`, and helpers in `capability.rs`.
- [ ] Implement `library init/add/list/check`.
- [ ] Commit as `feat: add capability library commands`.

### Task 2: Composer Name Resolution And Fit

- [ ] Add `resolve_pack_path(repo_root, ref)` that keeps existing paths working and falls back to library names.
- [ ] Add `AppCompositionPackFit` with name, path, status, issues, and command.
- [ ] Fill `packFit` from the same checks used by `app_change_plan_for_packs`.
- [ ] Add tests for compose-by-library-name and blocked duplicate fit.
- [ ] Commit as `feat: compose capability packs by library name`.

### Task 3: Admin Data And Console

- [ ] Add pack fit DTOs and parser tests in `platform-admin-data`.
- [ ] Regenerate contracts.
- [ ] Add Console model type and Launchpad rendering for pack fit.
- [ ] Add one model test.
- [ ] Commit Rust and Console changes separately.

### Task 4: Examples, Docs, Skills

- [ ] Add a fixture library file and fit state to the support-sla fixture.
- [ ] Extend fixture checks.
- [ ] Document the library flow in site docs and local skills.
- [ ] Commit examples/docs/skills.

### Task 5: Verification And PRs

- [ ] Run focused CLI tests for capability library and App Composer.
- [ ] Run `cargo test -p lenso-platform-admin-data launchpad_change_plan`.
- [ ] Run `just generated-check` and `just arch-check`.
- [ ] Run Console model tests and `pnpm typecheck:local`.
- [ ] Run examples fixture check and site typecheck.
- [ ] Push five v28 branches and open stacked PRs against `feat/capability-packs-v27`.
