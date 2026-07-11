# Adapt to service discovery infrastructure

Lenso will define logical Service References, Endpoint Resolver behavior, endpoint caching, and discovery evidence while integrating with static configuration, DNS, orchestrators, cloud discovery, or external registries through adapters. It will not operate its own distributed service registry, and established Data Plane traffic will resolve endpoints locally without a synchronous System Plane lookup.

## Consequences

- Business contracts and Service Clients do not embed deployment-specific addresses.
- Local development can use static resolution while production environments use platform-native discovery.
- The System Plane may coordinate topology changes, but resolvers retain last valid state across control-plane outages.
- Runtime Console can aggregate resolution, instance, and health evidence without becoming the registry.
- A future registry integration is an Endpoint Resolver adapter rather than a new Lenso control-plane cluster.
