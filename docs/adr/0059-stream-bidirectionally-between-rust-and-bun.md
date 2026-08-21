# ADR 0059: Stream bidirectionally between Rust and Bun

- Status: accepted
- Date: 2026-08-21
- Extends: ADR 0033, ADR 0035, ADR 0045, ADR 0047, ADR 0048, ADR 0050,
  ADR 0052, ADR 0056, ADR 0058
- Implements: #591

## Context

The request-only Bun bridge selected in ADR 0058 is not sufficient for a
stream Operation. A stream must preserve one logical session across an open,
ordered messages in both directions, independent half-close, cancellation,
bounded admission, and one terminal success or Domain Error. The portable
Kernel must not acquire a universal process or network wire format while Rust
and Bun remain able to act as either consumer or provider.

## Decision

Add a transport-neutral Kernel stream seam:

- `StreamCapability` describes the typed open request, bidirectional message,
  and Domain Error values.
- `NativeStreamEndpoint` opens an Adapter-owned `NativeStreamSession`.
- `NativeStream` exposes `send`, `receive`, `close_send`, and idempotent
  `cancel`; the Kernel applies the invocation deadline, cancellation token,
  generation state, and resolved bounded admission to every operation.
- `StreamEvent` distinguishes a message, the peer's half-close, and the one
  terminal outcome. Messages after a peer half-close, duplicate terminal
  outcomes, late frames, and invalid sequence transitions are protocol
  failures.

The Bun Adapter maps that seam to both existing prototype wires. Framed stdio
uses `stream_open`, `stream_call` with an explicit `action`, `stream_response`,
and `stream_cancel` frames. JSON-RPC uses `lenso.stream.open`,
`lenso.stream.send`, `lenso.stream.receive`, `lenso.stream.close_send`, and
`lenso.stream.cancel`. The exact Capability Descriptor adds a
`stream_operations` subset to the handshake; request and stream Operations are
never inferred from transport behavior.

Each opened session starts with bounded credit (16 by default). A send consumes
one credit and the accepted result replenishes it; Adapter pending calls and
Kernel stream admission are bounded as well. Stream call correlation IDs use
the JavaScript-safe integer range so Bun cannot round a Rust ID and misroute a
response. Cancellation retires the stream and drains or rejects late frames;
provider restart or process exit cancels existing sessions without replay and
creates a new session token for a later stable-handle open.

Rust provider bridges expose the same logical stream model through
`BunProviderStream`, so the conformance cases run in both directions: Rust
consumer → Bun provider and Bun consumer → Rust provider. The portable Kernel
and generated contract artifacts remain independent of Tokio, HTTP, stdio, and
process control.

## Consequences

Stream transport is now supported by the generated Rust and TypeScript
bindings and the native Rust and Bun Execution Adapters. Event fan-out,
subscriptions, universal stream persistence, and remote deployment remain out
of scope. Framed stdio remains a reproducible prototype wire while JSON-RPC
over loopback HTTP remains the production Bun default from ADR 0058.

The checked-in conformance evidence covers open Domain Errors, full duplex,
both half-close orderings, bounded stream admission, cancellation races,
terminal and late-frame rejection, provider restart behavior, and both wire
directions. A transport-specific failure remains a Runtime Failure or bounded
protocol failure; it is never replayed as a successful stream message.
