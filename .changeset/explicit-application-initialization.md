---
"@lenso/core": minor
"@lenso/engine": patch
"@lenso/cli": minor
---

Add explicit, opt-in application initialization with trusted host authorization,
existing configuration preflight, a durable first-owner/config transaction contract,
resumable activation and a startup readiness fence. The matching native CLI source
adds setup status/apply/activate; its release remains separate. Include a real
private SQLite first-run example, without enabling a bootstrap HTTP route or
installing platform secrets implicitly.
