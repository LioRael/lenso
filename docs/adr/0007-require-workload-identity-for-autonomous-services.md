# Require Workload Identity for Autonomous Services

Every Autonomous Service will authenticate callers through stable Service Principals proven by short-lived Workload Identity credentials, while user-initiated operations cross Service boundaries through bounded Delegated Actor Context rather than forwarded browser credentials. Each receiving Service authorizes locally from verifiable context, so identity enforcement does not depend on a synchronous System Plane lookup.

## Consequences

- Service identity is independent of IP addresses, hostnames, pods, or replica count.
- Credentials must support expiry and rotation.
- Delegation narrows audience, intent, permissions, and lifetime instead of copying the original actor credential.
- Lenso defines identity, delegation, propagation, authorization, and evidence contracts but integrates with local, orchestrator, cloud, or external credential issuers rather than operating its own certificate authority.
- Provider-mode host tokens remain a separate trust mechanism from Autonomous Service Workload Identity.
