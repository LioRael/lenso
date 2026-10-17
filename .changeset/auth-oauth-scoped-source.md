---
"@lenso/auth": minor
---

Add OAuth sources that require configured operation scopes while reusing ordinary Auth membership and policy checks in one verification. Bind OAuth sources to their human realm and recheck access expiry after permission reads. Distinguish malformed OAuth request JSON/UTF-8 (400) from backend failures (503), and document session renewal scheduling and expiry recovery.
