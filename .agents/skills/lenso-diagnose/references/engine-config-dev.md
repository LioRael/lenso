# Engine, instance config and dev

Read the [Engine guide](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/engine/README.md) for hook ownership, generated-file diagnostics and dev lifecycle. For instance configuration, use the [public core configuration docs](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/README.md#instance-configuration) and [Tasks config example](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/examples/tasks/src/config.ts).

## Assembly and configuration

- Locate the exact installed plugin object and its `requires` bindings. Use declaration/source locations for `duplicate-id`, `missing-dependency`, `cyclic-dependency`, `invalid-id` or `invalid-source`; another instance with the same service type is not interchangeable.
- Separate trusted TS config import/discovery from runtime instance preflight. Public `@lenso/core/config`, `/config/env` and `/config/file` support instance contracts and sources. Static descriptions show safe contracts, source IDs/locations and explicit env names, not resolved values or override history.
- Runtime preflight failures use phase `config`, top-level `config-invalid` and ordered safe causes: inspect instance, field path and source location for codes such as `config-source-failed`, `config-env-invalid` or `config-file-missing`. Failed sources do not silently fall back. Trace the app's declared precedence and converter in source without dumping contents.
- Report required env names as present/missing only. File/source availability does not establish configuration validity or grant access authority. There is no configuration center, subscription or hot-reload contract.

## Generated ownership and builds

Source lives outside `.lenso` and `dist`. Fix source/config/generator and regenerate through the existing launcher. `.lenso/.engine-files.json` tracks ownership; edited/unowned files, conflicting owners and path/symlink escapes are safety failures, not invitations to overwrite. Preserve any edits, identify their owner and ask before resolving a conflict; do not hand-edit output or delete the ownership ledger to bypass protection. Generation is not a filesystem transaction.

Inspect/call use canonical `lenso.config.ts`; Engine can use conventions and `lenso.engine.ts`. Check the selected build target/entry and installed public exports before attributing a runtime discovery failure to Engine config.

## Dev lifecycle

Check existing dev scripts and entry (`--entry` or `src/server.ts`), not build convention entry. A cycle generates and runs `beforeStart` before runtime startup; readiness follows application IPC and successful ready hooks. A listening-looking process without the expected `reportDevReady` signal can remain Starting. Static imports and explicit existing file/directory watches invalidate cycles; dynamic reads need the existing `context.watch` registration. Generated/cache paths are excluded.

Runtime termination precedes Engine LIFO cleanup on restart/shutdown. Diagnose startup/shutdown timeout and forced-termination reports separately from app readiness. Stop only the supervisor/runtime started by this task; another user's port is not a cleanup target.
