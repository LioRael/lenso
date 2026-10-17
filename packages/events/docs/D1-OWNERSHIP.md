# D1 candidate ownership and integration

Base: `7ecb55ddb53d71544a67f5cbc2786ee9af5f6d31` (origin/main in task17 source).
Task29 owns `src/d1.ts`, `test-d1/`, and this D1 documentation only.
PG integration owner owns shared contracts, dispatcher, entries, manifest and root lock.
No SQLite or PG public API change is part of this candidate.

- `initializeD1Events(binding): Promise<void>` explicitly creates the Events tables.
- `createD1EventStore(binding, { now? }): D1EventStore` borrows the D1 binding.
- `D1EventStore` maps each existing `EventStore` method to Promise of its return value.
  This is structurally compatible with an equivalent async Store shared by PG.
- No runtime dependency is added. D1 binding interfaces are structural and checked
  against published Cloudflare types in the local acceptance fixture.
- Do not pass this async store to the baseline synchronous `createEvents`.
  The PG owner's async service must await register/cancel/claim/acknowledge/retry;
  retain the synchronous SQLite API via the agreed separate async entry/overload.
- Shared manifest patch: add `./d1` types/import pointing at `dist/d1.d.ts` and
  `dist/d1.js`; add `src/d1.ts` to the Events build entry list. No root re-export or
  runtime dependency is needed. Root and plugin must keep native SQLite isolated.
