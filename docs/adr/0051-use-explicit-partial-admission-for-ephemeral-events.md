# Use explicit partial admission for ephemeral Events

Each Event subscriber binding will own an independent bounded in-memory queue.
A publish attempts admission to every currently bound subscriber and returns an
explicit per-binding result such as `accepted`, `unavailable`, or `exhausted`.
Admission across several subscriber queues is not atomic, and Kernel never
automatically retries a partial publish.

## Consequences

- FIFO ordering is guaranteed only for accepted Events on one
  publisher-to-subscriber binding. There is no global order across publishers
  or subscribers.
- `accepted` means that the Event entered the subscriber's volatile queue. It
  does not mean that a handler completed, that data was persisted, or that a
  crashed subscriber will receive the Event again.
- A slow or unavailable subscriber does not undo admission to other
  subscribers. The publisher receives the partial outcome and decides whether
  its domain requires another mechanism.
- Event handling is one-way. Handler Domain Errors are not routed back as a
  delayed response to the original publisher; a domain may define an explicit
  result Event, while operational failure may appear in Runtime Diagnostics.
- Applications that require atomic publication, durable acceptance, replay,
  consumer acknowledgements, or redelivery bind a broker, Outbox, or Durable
  Event Capability instead of assigning those meanings to Kernel Events.
