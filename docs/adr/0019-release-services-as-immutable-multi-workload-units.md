# Release Services as immutable multi-Workload units

An Autonomous Service will be released as one immutable, environment-independent Service Release that binds all API, worker, migration, and other Workload artifacts to the Service and Module versions, Contract Versions, Config Contract, migration and workflow compatibility, verification evidence, artifact digests, SBOM, provenance, signatures, rollout gates, and rollback metadata. Deployment adapters realize that release in an environment, and Promotion reuses the same digests rather than rebuilding.

## Consequences

- A Service Release, not a container image, is the atomic unit of compatibility and promotion.
- Environment-specific values and Secret values remain outside the release artifact.
- Docker, PaaS, Kubernetes, and Operator delivery consume the same release semantics.
- Release evidence can explain exactly which contracts, migrations, Workloads, and supply-chain artifacts entered an environment.
- Rollback decisions can reason about the whole Service boundary instead of independent container tags.
