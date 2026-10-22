---
"@lenso/auth": patch
---

Reject OAuth exchanges that expire during permission reads or CAS acknowledgements. Cap access-token expiry by the grant deadline and report the remaining lifetime in `expires_in`, without retrying consumed code or refresh writes.
