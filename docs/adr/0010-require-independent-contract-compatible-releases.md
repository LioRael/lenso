# Require independent, contract-compatible releases

Autonomous Services must be independently releasable without a system-wide lockstep deployment. Compatible contract additions may evolve an existing Contract Version, while breaking HTTP, gRPC, or Event Contract changes require a parallel major version, Consumer and Provider Compatibility Verification, an explicit deprecation window, and evidence that no active consumer remains before Contract Retirement.

## Consequences

- Contract artifacts are versioned independently from Service implementation releases.
- Release planning includes compatibility diffs and affected Consumer evidence.
- CI and release policy provide a can-I-deploy gate for the intended System graph.
- Providers support old and new breaking Contract Versions in parallel during migration.
- The System Plane coordinates compatibility and retirement evidence but does not negotiate contracts on every Data Plane request.
