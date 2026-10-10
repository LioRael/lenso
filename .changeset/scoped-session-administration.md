---
"@lenso/auth": minor
---

Add explicitly authorized session administration with bounded realm-scoped pagination, safe detail DTOs, and revision-checked revocation. Native SQLite, PostgreSQL, and D1 session stores expose the optional administration capability while existing credential-holder session APIs remain compatible. Administrative revocation requires an acknowledged audit intent and reports uncertain effects for reconciliation.
