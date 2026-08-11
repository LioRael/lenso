## @lenso/service-kit@0.1.5

## 0.2.0

### Minor Changes

- 1894bc8: Expose an optional loopback-only Provider Core identity route with exact local
  bearer validation for Console enrollment checks.

## 0.1.6

### Patch Changes

- 25f01e8: Publish the evolved TypeScript Service Kit contract and generated service schema.

### Fixed

Build the Service Kit public entrypoints before the reviewed publisher seals its
npm archive, and reject any future archive that omits a declared entrypoint.

## @lenso/service-kit@0.1.4

### Maintenance

Retry the current Foundation and Service Kit package set after reviewing the
new System Plane dependencies and moving that review into plan generation.

## @lenso/service-kit@0.1.3

### Maintenance

Rebind the current Foundation and Service Kit package set to the reviewed
release-mode contract after the previous shadow publisher stopped before
preflight and registry access.

## @lenso/service-kit@0.1.2

### Features

Publish the Framework-owned Service authoring kit with Provider V1 delivery,
typed Module manifests, signed enrollment, and System Plane integration.

## @lenso/service-kit@0.1.1

### Fixes

Validate the complete npm Shadow release chain with authoritative preflight binding.
