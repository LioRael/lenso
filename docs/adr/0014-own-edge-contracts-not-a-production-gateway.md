# Own Edge Contracts, not a production Gateway

Lenso will define and verify Edge Contracts and generate environment-specific Gateway configuration, but it will not implement the production traffic proxy, TLS termination, WAF, or global routing data plane. Provider traffic may continue through the current Host proxy, while Autonomous Service traffic uses mature local, orchestrator, self-hosted, or cloud Gateway implementations through adapters.

## Consequences

- Public exposure, versions, authentication, cross-origin behavior, rate intent, and deprecation are explicit system contracts.
- Internal-only Service operations remain distinguishable from edge APIs.
- Lenso may ship a lightweight local development Gateway without making it the production recommendation.
- Kubernetes Gateway API, Envoy, Traefik, Kong, and cloud gateways are potential adapters rather than platform dependencies.
- Runtime Console can correlate Edge policy and traffic evidence without proxying the traffic.
