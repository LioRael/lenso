# Keep Console out of deployment authority

> **Status: superseded for vNext.** Retained as historical and v0.3.x
> maintenance context. [ADR 0030](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
> and ADRs 0031 onward are normative for vNext.

Lenso will own System Topology, Console Management Bindings, Service enrollment, readiness contracts, Module Surface composition, and provider-neutral Workload control contracts. Console connects an existing System and may request capability-negotiated `suspend`, `resume`, `restart`, or `scale` operations through a Workload Control Adapter, but external deployment authorities continue to own Workload creation, release selection, upgrade, replacement, and deletion. The user-facing lifecycle is `Connect / Status`, not deployment-style `Plan / Apply`.

## Consequences

- Console does not model dev, staging, or production environments; platform differences remain behind Adapter capabilities and authority policy.
- A Console-owned Management Binding replaces an environment-specific System Profile.
- Local tooling or an external deployment system creates Workloads before Console controls them; local startup may automatically connect the resulting System.
- Systems without a Workload Control Adapter remain connectable and observable, but their Workload controls are unavailable.
- Console and its active Workload Control Adapter remain externally managed and cannot control their own lifecycle.
- Workload operations are typed, asynchronous, least-privileged, auditable, and never fall back to arbitrary shell, SSH, or stored infrastructure credentials.
- Kubernetes support remains an optional Adapter integration rather than a required Lenso runtime or Console-owned controller.
- Planning, idempotency, and resumable journals may exist internally without becoming deployment concepts in the product model.
