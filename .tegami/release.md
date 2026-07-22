---
packages:
  lenso-module-story:
    type: patch
---

### Fixes

Publish the Story module with its SQLx derive feature declared directly so the
isolated Cargo package verifies without relying on workspace feature unification.
