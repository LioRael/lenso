---
"@lenso/auth": patch
---

Reject OAuth exchanges that expire during permission reads or CAS acknowledgements. Cap access-token expiry by the grant deadline and report the remaining lifetime in `expires_in`, without retrying consumed code or refresh writes.

Preserve RFC7009 success responses for missing, expired or already revoked tokens. Reconcile conflicting or unacknowledged revocations with one authoritative read, without replaying the write or claiming success while a token remains live.
