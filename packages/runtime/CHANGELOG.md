# @lenso/runtime

## 0.2.0

### Minor Changes

- f0cab26: Add schema-derived Runtime service authoring and deferred typed handler selection.
  Generate the existing v1 contract JSON through Engine discovery, generation and
  watching, with path-specific rejection of unsupported schema constraints.

### Patch Changes

- 7664344: Accept Engine 0.6 and 0.7 in optional integrations after whole-workspace compatibility verification. Keep these packages pending for a separately reviewed release.
- c60317c: Add optional Runtime integration with typed clients, explicit export activation
  after application setup, bounded cleanup, Engine companions, and authorized
  read-only Manage diagnostics.
- Updated dependencies [7664344]
- Updated dependencies [bfd8133]
  - @lenso/manage@0.5.4
  - @lenso/core@0.4.0
  - @lenso/engine@0.7.1
