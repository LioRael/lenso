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
- `GET /runtime/event-deliveries`

Successful business requests and background function/event outcomes are
persisted as local Story Segments. Module registrations inject business routes,
runtime functions, event handlers, and migrations. The Service-owned Worker
claims its Store's queues and transactional Outbox, persists retry state and
health locally, and releases only its own claims during deterministic shutdown.
This crate does not select concrete Modules or use the Host/Provider boot facade.

The public `TransportAdapter` boundary carries authoritative Event Envelopes
through protocol-neutral publish, receive, acknowledgement, negative
acknowledgement, health, and diagnostic operations. `LocalTransportAdapter`
uses an injected PostgreSQL Store as the dependency-free System Sandbox
transport: it requires no broker, Kubernetes, service mesh, Runtime Console,
or System Plane. A Service records publication intent through
`ServiceEventPublisher` in the same Store transaction as its business write;
the relay and consumer helpers retain Service-owned Outbox, Inbox, terminal
delivery evidence, and Module-owned business effects locally. Stable failure
classification and controlled retry schedules persist beside Inbox history;
poison and exhausted deliveries move to durable dead-letter state without
blocking later healthy events.

`NatsJetStreamTransportAdapter` is the first production implementation. It
binds an operator-provisioned stream and durable pull consumers, receives an
operator-created credential-bearing client, and retains adapter diagnostics in
the injected Service Store. It does not provision or clean up production NATS
infrastructure. The local and JetStream implementations run the same
conformance suite, including interruption redelivery and the unchanged
authoritative support Event Envelope behavior.

Authenticated event receivers use `ServiceEventWorkloadIdentity` with
`consume_service_events_once_at`. The receiver verifies the
Event Envelope's signed, audience-limited Service Principal and its
authenticated Transport Adapter binding before Module behavior runs. Invalid
identity is recorded as an unauthorized terminal delivery. Endpoint, process,
replica, region, and Failure Domain metadata remain operational evidence and
are never used as Service identity.

Dead-letter operator workflows return stable, versioned command results.
`inspect_dead_letters` returns deterministic evidence;
`plan_dead_letter_replay` and `plan_dead_letter_cleanup` are non-mutating dry
runs; replay preserves the original Event Envelope while recording a distinct
delivery attempt. Production replay requires explicit production approval, and
all destructive cleanup requires explicit approval. Cleanup removes only
resolved dead-letter records while retaining Inbox deduplication state,
delivery evidence, and replay audit records.
