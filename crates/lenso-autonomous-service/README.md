# lenso-autonomous-service

Host-independent runtime composition for one `lenso.service.v2` Autonomous
Service.

The crate validates a Service definition against explicit runtime ownership,
applies migrations to an injected Service-owned PostgreSQL Store, and composes
API, Migration, and Worker Workloads under one Service identity. It exposes:

- `GET /health/live`
- `GET /health/ready`
- `GET /health/startup`
- `GET /runtime/story-segments`

Successful business requests and background function/event outcomes are
persisted as local Story Segments. Module registrations inject business routes,
runtime functions, event handlers, and migrations. The Service-owned Worker
claims its Store's queues and transactional Outbox, persists retry state and
health locally, and releases only its own claims during deterministic shutdown.
This crate does not select concrete Modules or use the Host/Provider boot facade.
