# Stream and Event contracts

Read this reference only when the Capability contains a stream or event
Operation. Verify that the selected generator and every target Execution
Adapter currently support the interaction; Descriptor support alone does not
make an Adapter authoring surface complete.

## Stream

Use a stream for one ordered bidirectional session with bounded flow,
independent half-close, cancellation, and exactly one explicit terminal
success, Domain Error, or Runtime Failure.

```json
{
  "id": "example.conversation@1",
  "version": "1.0.0",
  "portable": true,
  "operations": [
    {
      "name": "chat",
      "interaction": "stream",
      "request_schema": "schemas/chat-open.schema.json",
      "response_schema": "schemas/chat-message.schema.json",
      "domain_error_schema": "schemas/chat-error.schema.json"
    }
  ]
}
```

Define the opening input and flowing message shape deliberately. The generated
session must demonstrate both half-close directions, backpressure, terminal
outcome, deadline/cancellation, late-frame handling, and bounded buffers.
Transport disconnect is not the terminal business result.

## Event

Use an event for volatile fan-out to zero or more subscribers. Each subscriber
has independent bounded admission and publication reports partial outcomes.

```json
{
  "id": "example.notifications@1",
  "version": "1.0.0",
  "portable": true,
  "operations": [
    {
      "name": "notify",
      "interaction": "event",
      "request_schema": "schemas/notify-event.schema.json",
      "response_schema": "schemas/notify-response.schema.json",
      "domain_error_schema": "schemas/notify-error.schema.json"
    }
  ]
}
```

Event does not promise persistence, replay, redelivery, global ordering, or
exactly-once delivery. Model those product requirements through an owning
stateful Module, Outbox, broker Adapter, or durable Capability rather than
changing Kernel Event semantics.

## Proof matrix

For every selected runtime combination, prove:

| Interaction | Required evidence |
| --- | --- |
| Stream | open rejection, two-way data, both half-closes, bounded send, cancellation, terminal success/Domain/Runtime result, late frame |
| Event | zero subscribers, all admitted, partial admission, all rejected/resource exhausted, cancellation, deterministic provider order |

Use the current stream/event fixtures and cross-runtime tests in
`LioRael/lenso-protocols`, `LioRael/lenso`, and
`LioRael/lenso-bun-adapter`. If an SDK rejects the interaction, record that as
the supported boundary rather than bypassing it with handwritten wire code.
