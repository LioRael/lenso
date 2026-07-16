# Select NATS JetStream as the first production Transport Adapter

Lenso selects NATS JetStream as its first production Transport Adapter. The
selection is deliberately adapter-specific: Event Contracts, Event Envelopes,
Module behavior, Inbox idempotency, Outbox publication, and Service-owned
delivery evidence remain protocol-neutral.

## Decision record

| Dimension | NATS JetStream | RabbitMQ quorum queues | Apache Kafka |
| --- | --- | --- | --- |
| Delivery semantics | Durable pull consumers, explicit per-message acknowledgement, negative acknowledgement, acknowledgement deadlines, and server-managed redelivery directly match the existing Transport Adapter boundary. | Durable replicated queues and explicit acknowledgement also match, but delayed retry and dead-letter topology require more queue/exchange policy. | Durable logs and committed consumer offsets are strong, but per-message negative acknowledgement and independently delayed redelivery require application-side retry topics or offset coordination. |
| Operational burden | One small server binary supports local proof; production can add stream replication without changing the adapter contract. | A good fit for teams already operating AMQP, but quorum queues, exchanges, bindings, and dead-letter policies introduce more topology for this first support flow. | Strongest fit for high-throughput log platforms, but brokers, partitions, consumer-group operations, and retention planning exceed the target team's current small-service needs. |
| Target-team fit | Best match for teams evolving from a modular monolith to a small service system: low local friction and a clear path to replicated operation. | Reasonable second adapter for AMQP-oriented organizations. | Deferred until a real ordered-log, replay, or throughput requirement justifies it. |
| Security integration | TLS, mutual TLS, NKeys/JWT accounts, and subject permissions can be supplied by the operator-created client without entering Module configuration. | Mature TLS and identity/permission controls, but they would add an AMQP-specific credential and virtual-host topology surface. | Mature TLS/SASL and ACLs, with greater cluster and identity integration overhead. |
| Failure behavior | Publish acknowledgements plus durable explicit-ack consumers provide at-least-once delivery; an unacknowledged delivery is redelivered after interruption. Service Inbox idempotency remains responsible for exactly-once business effects. | Comparable at-least-once behavior; quorum queues improve availability, while retry/dead-letter behavior is more topology-dependent. | At-least-once processing is natural, but a single failed record can hold a partition unless retry handling moves outside the primary consumer loop. |
| Testability | The same Rust client and real `nats-server -js` process run the shared adapter conformance suite in CI. | Real-environment tests are feasible but require more declarations to reproduce queue policies. | Real-environment tests require the heaviest fixture and make focused interruption proofs slower. |

The semantics above follow the upstream descriptions of
[JetStream consumers](https://docs.nats.io/nats-concepts/jetstream/consumers),
[RabbitMQ quorum queues](https://www.rabbitmq.com/docs/quorum-queues), and
[Kafka delivery semantics](https://kafka.apache.org/documentation/#semantics).

## Boundaries and consequences

- The production adapter binds an already provisioned stream and durable pull
  consumers. It does not create, update, or delete production infrastructure.
- Composition resolves credentials and constructs the NATS client. Production
  deployments should use TLS plus operator-managed NKeys/JWT or mutual TLS and
  least-privilege subject permissions; credential values never enter Lenso
  configuration state.
- Broker subjects, stream names, durable consumer names, delivery metadata, and
  client types stay inside adapter composition. Modules continue to publish the
  authoritative `lenso.event-envelope.v1` value to a logical consumer ID.
- JetStream provides at-least-once transport. Service-owned Inbox state remains
  authoritative for preventing a repeated business effect after redelivery.
- Retained adapter diagnostics are written to the injected Service Store so
  restart and broker interruption do not erase Lenso delivery evidence.
- CI may create and delete a uniquely named, short-lived JetStream stream only
  when `LENSO_NATS_TEST_INFRASTRUCTURE_APPROVED=true` is explicit. Production
  credentials, production topology mutation, and production cleanup remain
  separate operator approval boundaries.
- The local PostgreSQL adapter remains the dependency-free System Sandbox path;
  local success is not presented as production equivalence.
