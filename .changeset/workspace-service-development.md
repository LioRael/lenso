---
"@lenso/engine": minor
"@lenso/cli": minor
---

Add explicit development workspaces for native TypeScript applications and
separately built services. Start dependencies after readiness, report candidate
and retained-instance state, rebuild affected services, and close dependents
before their dependencies. Expose the workspace through `lenso dev --workspace`
without changing single-application development or business operation allowlists.
