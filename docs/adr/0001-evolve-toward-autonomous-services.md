# Evolve toward autonomous services

Lenso will keep its current Host-managed Provider mode as the low-friction path for extracting linked Modules, while treating Autonomous Services coordinated through a federated System Plane as the long-term microservice direction. A Service is an independently delivered logical boundary rather than one process; API, worker, and migration processes are Workloads of that Service. This preserves the modular-first adoption path without making the current Host the permanent owner of every service's runtime, persistence, lifecycle, and release cadence.

## Considered Options

- Keep every Provider subordinate to one Host and continue calling each provider process a Service. This preserves the current model but limits Lenso to a remote-module platform and conflates logical ownership with process topology.
- Replace the current model with autonomous peer services immediately. This reaches the target architecture faster but breaks the gradual linked-to-service path.
- Support both as an explicit evolution path. This adds two operating modes but lets boundaries become autonomous only when the operational need is real.
