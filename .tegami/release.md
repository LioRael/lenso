---
packages:
  lenso-contracts:
    type: patch
  lenso-service:
    type: patch
---

### Fixes

Restore the complete reviewed M6 Cargo dependency closure so every unpublished
foundation, module, and host package is built and verified in one shadow plan.
