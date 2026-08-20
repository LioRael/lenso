# Generate and conformance-test portable bindings

A portable Capability contract package will keep its Descriptor and Schema as
the only authoritative source and generate Rust and TypeScript value types,
consumer clients, and provider skeletons. The build selects one exact
Descriptor version, and each cross-runtime Execution Adapter confirms the
Capability series, exact version, and Operation table during preparation before
it accepts calls.

## Consequences

- An Adapter handshake mismatch fails Module preparation. Kernel does not wait
  for the first business call, compare full schemas, or synthesize a runtime
  conversion between versions.
- Generated artifacts may be published for normal package-manager ergonomics,
  but they carry source-version metadata and cannot override their Descriptor
  contract.
- Handwritten native convenience APIs may wrap generated bindings, but a second
  handwritten portable Interface is unsupported.
- A transport-independent black-box conformance suite covers value round trips,
  known and unknown Domain Errors, Runtime Failures, deadlines, cancellation,
  admission backpressure, bidirectional stream half-close, and protocol
  violations for both consumer and provider sides.
- The conformance behavior is fixed before selecting the first Bun wire
  implementation. Framed stdio and a mature RPC stack must be evaluated against
  the same suite rather than shaping different Capability semantics.
