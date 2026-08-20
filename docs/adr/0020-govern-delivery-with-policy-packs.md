# Govern delivery with Policy Packs

> **Status: superseded for vNext.** Retained as historical and v0.3.x
> maintenance context. [ADR 0030](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
> and ADRs 0031 onward are normative for vNext.

Lenso will evaluate versioned, environment-scoped Policy Packs for Service planning, release, Promotion, and high-risk operational actions. The same deterministic rules run locally, in CI, and through the System Plane and produce explainable Policy Evidence; they govern whether an action may proceed but do not become a centralized authorization dependency for ordinary Data Plane requests.

## Consequences

- Production policies can require contract compatibility, safe resilience declarations, signed digest-pinned artifacts, provenance, migration discipline, identity, tenancy, Secret References, health gates, Environment Verification, and dependency readiness.
- Development environments may use intentionally less strict Policy Packs.
- Every failure identifies the violated rule, supporting evidence, and a concrete remediation path.
- Repository-specific CI invokes the policy model instead of recreating it through unrelated shell checks.
- Policy Pack versions become part of release and Promotion evidence.
