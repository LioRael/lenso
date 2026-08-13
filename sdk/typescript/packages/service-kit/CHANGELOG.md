## @lenso/service-kit@0.1.5

## 0.5.1

### Patch Changes

- d0431aa: Emit canonical `lenso.module-manifest.v1` fields from `defineModule` and keep legacy authoring aliases out of serialized Service manifests.

## 0.5.0

### Minor Changes

- 5dec5a7: Expose exact Provider Module Releases for local Host composition and pass the authenticated actor into Provider HTTP handlers.

## 0.4.0

### Minor Changes

- 65c8d65: Serve the exact `lenso.provider.v1` descriptor, health, invocation, recovery,
  and acknowledgement protocol from the standard Service Kit server.

## 0.3.0

### Minor Changes

- 28e73ac: Add RFC 9457 Problem Details helpers and return `application/problem+json` for Service Kit HTTP errors.

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
