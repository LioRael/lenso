# lenso-autonomous-service

Host-independent runtime composition for one `lenso.service.v2` Autonomous
Service.

The crate validates a Service definition against explicit runtime ownership,
applies migrations to an injected Service-owned PostgreSQL Store, and exposes:

- `GET /health/live`
- `GET /health/ready`
- `GET /health/startup`
- `GET /runtime/story-segments`

Successful business requests are persisted as local Story Segments. The
business router and module migrations are injected by the Service composition
root; this crate does not select concrete Modules or use the Host/Provider boot
facade.
