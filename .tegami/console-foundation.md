---
packages:
  lenso-contracts:
    type: patch
  lenso-module-management:
    type: patch
  lenso-platform-core:
    type: patch
  lenso-platform-http:
    type: patch
  lenso-platform-module:
    type: patch
  lenso-platform-provider:
    type: patch
  lenso-platform-runtime:
    type: patch
  lenso-service:
    type: patch
---

### Features

Publish the Linked Module management, Provider V1, Console Bridge authority, and
signed Service enrollment contracts required by the isolated System Plane.

### Breaking changes

Remove the legacy Remote Module and in-host Console package declarations from
the active Module contract. Remote processes are Services and Module sources are
Linked.
