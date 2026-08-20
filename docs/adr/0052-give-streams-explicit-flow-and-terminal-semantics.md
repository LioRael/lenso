# Give Streams explicit flow and terminal semantics

A stream Operation will establish one bidirectional, ordered session with
independent half-close, bounded flow control, cancellation, and exactly one
terminal outcome. Opening may fail with a Domain Error or Runtime Failure; an
established stream ends with success, a Domain Error, or a Runtime Failure
rather than relying on transport disconnection as its result.

## Consequences

- Consumer and provider may close their sending directions independently while
  continuing to receive. Closing both directions does not replace the explicit
  terminal outcome.
- Sending observes bounded credit or window capacity and waits under the
  invocation deadline and cancellation signal. Kernel and Adapters never use
  unbounded buffering or silently drop stream messages.
- Cancellation is idempotent. If no terminal outcome already won the race, the
  invocation completes as `Cancelled`; late frames are drained, rejected, or
  treated as a protocol violation according to the Adapter state machine and
  are never delivered to application code.
- The Resolved App Plan and selected Adapter impose hard limits on message and
  frame sizes, concurrent streams, and buffered bytes. Supported local calls
  report `ResourceExhausted` on admission limits, while malformed or impossible
  wire frames report `ProtocolViolation`.
- The portable conformance model defines logical handshake, open, data,
  half-close, cancel, terminal, and Event-admission behavior. Each Portable
  Invocation Adapter maps that model to its own framing and connection-level
  codec; Kernel owns no universal wire byte format.
- The first Bun codec remains an evidence-based prototype decision. It is fixed
  per selected Adapter connection rather than negotiated independently for each
  invocation.
