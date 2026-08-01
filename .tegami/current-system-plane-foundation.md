---
packages:
  lenso-autonomous-service: patch
  lenso-contracts: patch
  lenso-module-management: patch
  lenso-platform-core: patch
  lenso-platform-http: patch
  lenso-platform-module: patch
  lenso-platform-module-management: patch
  lenso-platform-provider: patch
  lenso-platform-runtime: patch
  lenso-platform-runtime-observability: patch
  lenso-platform-runtime-operations: patch
  lenso-platform-system-plane: patch
  lenso-platform-testing: patch
  lenso-service: patch
---

### Features

Publish the current Linked Module, Provider V1, signed Service enrollment, and
independent System Plane composition contracts from the completed architecture.

### Breaking changes

Remote Module is not an active loading source. Independently running code is a
Service, and current Modules are Linked into their owning Host.
