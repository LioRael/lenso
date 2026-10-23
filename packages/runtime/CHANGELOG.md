# @lenso/runtime

## 0.2.0

### Minor Changes

- f0cab26: Add schema-derived Runtime service authoring and deferred typed handler selection.
  Generate the existing v1 contract JSON through Engine discovery, generation and
  watching, with path-specific rejection of unsupported schema constraints.
- c5873de: Generate TS and Rust bindings together with the existing v1 contract through
  runtimeContractPlugin, using the stable Runtime SDK generator export and Engine
  ownership. Requires a Runtime SDK candidate providing @lenso/runtime-sdk/generate.

### Patch Changes

- d5dc7ab: Retain dependency updates deferred by scoped preparation of @lenso/cli, @lenso/core, @lenso/engine, @lenso/manage, @lenso/mcp.
  Review and publish these dependent packages separately; their versions were not advanced.
- 88fcd54: Retain dependency updates deferred by scoped preparation of @lenso/auth, @lenso/cli, @lenso/engine, @lenso/manage, @lenso/mcp, @lenso/realtime, @lenso/scheduler, @lenso/storage, @lenso/tasks.
  Review and publish these dependent packages separately; their versions were not advanced.
- 7664344: Accept Engine 0.6 and 0.7 in optional integrations after whole-workspace compatibility verification. Keep these packages pending for a separately reviewed release.
- c60317c: Add optional Runtime integration with typed clients, explicit export activation
  after application setup, bounded cleanup, Engine companions, and authorized
  read-only Manage diagnostics.
- Updated dependencies [7664344]
  - @lenso/manage@0.6.1
