---
"@lenso/authorization": patch
"@lenso/storage": patch
---

Index roles within each exact-scope permission calculation without caching authorization decisions across requests or revisions.

Bound local listing metadata retention to one page plus a cursor candidate while preserving full-store integrity checks. Limit S3 listing metadata reads to two concurrent HEAD calls, preserving list order and settling started reads before returning failures.
