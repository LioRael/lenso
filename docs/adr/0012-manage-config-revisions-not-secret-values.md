# Manage Config Revisions, not Secret values

Lenso will own Service Config Contracts, immutable Config Revisions, validation, impact analysis, staged activation, rollback, and drift evidence while storing only Secret References. Each Service resolves sensitive values locally through environment-specific Secret Providers and retains its last valid configuration so System Plane unavailability does not interrupt established Data Plane execution.

## Consequences

- Config Contracts declare type, scope, sensitivity, mutability, and restart or hot-activation requirements.
- Config changes are versioned release inputs rather than untracked mutable state.
- Local environment files, orchestrator secrets, Vault, and cloud secret managers are provider integrations.
- Runtime Console exposes configuration provenance, revision, drift, and rotation status without exposing Secret values.
- Lenso does not become a certificate authority or Secret storage product.
