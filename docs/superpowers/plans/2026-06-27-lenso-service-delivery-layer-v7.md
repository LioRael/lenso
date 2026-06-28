# Lenso Service Delivery Layer V7 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Lenso services verifiable, upgradeable, observable, and deployable without turning the host into a service mesh.

**Architecture:** Keep the host as the control plane for auth, capability checks, queues, retries, outbox, Runtime Story, and Technical Operations. Treat a service as a remote provider process that owns one or more modules and publishes one service contract. Build V7 around one shared contract schema consumed by SDKs, CLI checks, Console service detail, upgrade diff, and deployment export.

**Tech Stack:** Rust 2024, serde/serde_json, jsonschema-compatible JSON Schema, clap, reqwest, TypeScript, Vitest, React, TanStack Query/Router, pnpm.

---

## Scope

V7 adds the delivery layer around the V6 service/provider surface:

- shared `lenso.service.json` schema;
- stronger `lenso service check`;
- Rust and TypeScript SDK validation helpers;
- Console service detail model;
- service upgrade/diff/rollback state;
- deployment export fragments;
- Rust and TypeScript proof services.

Do not add service discovery, gateway routing, service mesh, distributed transactions, schema registry, Kubernetes operators, or automatic trust.

## Batch 1: Contract And Check

- [ ] Add a shared service contract JSON Schema.
- [ ] Package the schema where CLI, Rust SDK, and TS SDK can consume it.
- [ ] Add minimal Rust and TypeScript validation helpers.
- [ ] Make `lenso service check` validate local files and remote manifests against the schema.
- [ ] Keep old service manifests accepted when they map cleanly to V6 fields.

## Batch 2: Author Test Harness

- [ ] Add `lenso service check --serve-command <cmd>` to start a service, wait for ready, fetch the manifest, and stop the child process.
- [ ] Add route/action/runtime/event probes only where the manifest declares a safe target.
- [ ] Print service-author language: missing env, manifest unreachable, endpoint unhealthy, capability mismatch.

## Batch 3: Console Service Detail

- [ ] Add a provider detail route or drawer from `/services`.
- [ ] Show provider metadata, provided modules, config/env hints, install receipt, compatibility, health history, recent calls, Runtime Story links, and Technical Operations links.
- [ ] Attach one next action to each degraded state.

## Batch 4: Upgrade, Diff, Rollback

- [ ] Snapshot installed service manifests in install receipts.
- [ ] Add `lenso service diff <service> <manifest>`.
- [ ] Add `lenso service upgrade <service> <manifest>`.
- [ ] Add `lenso service rollback <service>`.
- [ ] Warn when removing a service still leaves installed modules depending on it.

## Batch 5: Deployment Export

- [ ] Extend `lenso service export` with compose, systemd, dockerfile, and env outputs.
- [ ] Generate fragments from `localProcess`, env schema, and health endpoints.
- [ ] Keep deployment snippets static and reviewable; no orchestration runtime.

## Batch 6: Multi-Language Proof

- [ ] Keep the TypeScript `support-suite-provider` as the broad service-suite proof.
- [ ] Add one Rust provider proof with create, check, install, Console observation, upgrade diff, and export coverage.

## First Slice

Start with Batch 1. It unlocks every later batch and is the smallest durable foundation:

```sh
cargo test --locked -p lenso-service
cd /Users/leosouthey/Projects/framework/lenso-runtime-console && pnpm --filter @lenso/service-kit test
cd /Users/leosouthey/Projects/framework/lenso-cli && cargo test --locked service
```
