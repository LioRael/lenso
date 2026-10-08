---
name: lenso-diagnose
description: Troubleshoot existing Lenso assembly, configuration, build/dev, Auth and durable tasks; inspect authorized runtime status and plan or perform explicitly authorized recovery. Not for adding business interfaces, SQL/migrations, secret inspection, bulk retries or release approval.
---

# Diagnose existing Lenso applications

Follow the symptom, not a mandatory checklist. Use existing app operations and public tooling; if the needed status/recovery entry is absent, report the interface gap rather than creating one.

## Find the failing boundary

1. Identify the app root, relevant manifest scripts and installed Lenso package versions, runtime (Bun or Workers), and exact plugin/app instance. Record the failing command or entry and intended environment. Check only required environment-key **presence**, never values, full configs or credentials.
2. When CLI is the failing or selected diagnostic entry, read its installed `lenso help --json` through the app's existing launcher. For a declared operation, use `lenso inspect <plugin-id> <method> --root <app> --json`; omit selectors only when locating the declaration. Otherwise use the actual entry's help/description. Read the operation's description, shared input schema, source and semantic metadata before considering a call. Inspection is static, not health/status.
3. Classify by diagnostic `code`, actual `phase`, source and ordered causes, not exit code alone. Separate arguments/input, discovery/assembly, runtime config preflight, setup, invoke, cleanup, output and build. Keep Engine config failures distinct from runtime instance config. Preserve both invocation and cleanup failures; an output/cleanup failure can follow a committed business effect.
4. Choose the smallest relevant check or source fix. For an authorized existing operation, use `lenso call <plugin-id> <method> --root <app> --stdin --json` or `--input-file <file>` with its actual input schema. Assess **all installed setup** before calling, including read operations. After uncertain results, query authorized state or report uncertainty instead of replaying.
5. Verify the changed boundary using the app's existing focused test/typecheck or command. Rebuild changed framework packages before consumers because exports resolve to `dist`; do not typecheck consumers while a build clears that same output. Report observed phase/code/source, evidence, changes, checks run and remaining uncertainty; name any missing authorized interface.

Finite CLI commands return one `schemaVersion: 1` JSON envelope on stdout; logs go to stderr. `dev --json` is unsupported. Metadata (`effect`, `destructive`, `retry`, `cancellation`) describes intent, not permission, idempotency, signal propagation or rollback. `outputDescription` is prose, not an output schema.

## Execution and authority gates

- `inspect` imports **trusted** canonical `lenso.config.ts`, executes top-level code and schema converters, but runs neither application nor Engine setup. It is not a sandbox. `inspect`/`call` do not load `lenso.engine.ts`.
- `check`/`generate`/`build` execute trusted **Engine** setup, not application runtime setup. `generate` and `build` write owned outputs. `dev` also starts the runtime. Assess those effects before running a diagnostic command.
- `call` validates input before startup, then resolves every installed configured instance before any business setup and starts/stops the whole app. Preflight sources can perform I/O. Inspection/generation do not invoke configuration `source.read`, although imports/converters still execute.
- Work within the existing entry's verified identity and object/tenant policy. Use trusted launch credentials, not business JSON or a fabricated actor. Keep authorization checks intact.
- This skill grants no publication, deployment, production DB/migration/write authority, secret queries, arbitrary SQL, bulk retry or release approval. Recovery mutations require explicit authorization for the target/action; stop when authority, eligibility or effects are unclear.
- Close only resources/processes/ports owned by this task. Cleanup is not compensation for external side effects.

## Load only the relevant branch

- Missing/duplicate/cyclic instances, config preflight, Manage admission/binding, generated-file protection or dev readiness/restarts: [Engine, config and dev](references/engine-config-dev.md).
- Authentication rejection, audience/instance mismatch, membership or object ownership: [Auth](references/auth.md).
- Stuck/failed/cancelled jobs or retry requests: [Task status and recovery](references/tasks.md), **before any recovery mutation**.
- Opaque errors, missing traces/logs, MCP connection/cancellation or Workers lifecycle: [Observe and platform boundaries](references/observe-platform.md).

## Public source

Use the [CLI guide](https://github.com/LioRael/lenso/blob/main/docs/CLI.md) for command/diagnostic semantics, not guessed Manage or Observe commands. Manage supplies an optional instance-bound SDK/agent/oRPC entry, not a generic management CLI or recovery journal. Runtime status comes through explicitly selected, authorized operations; no Observe query CLI is provided.

Links follow repository `main`. Check actual installed package exports and build provenance: equal version strings do not prove equivalence to current source, and merged features may not yet be published. Use the installed contract when documentation is newer. If a referenced entry is absent, report the availability gap; do not invent fallback APIs/glue or install tooling. A framework checkout is not required: read public docs/examples and installed public exports; relative links refer only to this skill's own references.
