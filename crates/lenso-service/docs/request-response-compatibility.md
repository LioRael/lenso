# Request-response compatibility

`evaluate_request_response_compatibility_in_system` is the authoritative evaluator intended for
public Rust and CLI diff surfaces. It accepts canonical JSON operation shapes
derived from versioned OpenAPI documents or Protobuf descriptors and returns
one of exactly four categories: `safe`, `needs_attention`, `breaking`, or
`blocked`.

The evaluator applies compatibility in the direction of each relationship:

- A new Producer must continue accepting requests sent by existing Consumers.
- A new Producer must continue returning responses understood by existing
  Consumers.
- Unknown formats, missing schemas, unresolved Producer or Consumer
  references are `blocked`, never `safe`.
- Changes that remain wire-compatible but may change generated source or JSON
  behavior, such as a Protobuf field rename, are `needs_attention`.
- Required request additions, removed response fields or operations, and field
  type changes are `breaking`.

Every artifact pair declares distinct before/after versions. Every result carries the explicit contract kind, contract id, changed version,
sorted Producer and Consumer references, stable reason codes, and a next action.
Provider Protocol results use `provider_protocol_*` reason codes; Autonomous
Service Contract results use `service_contract_*` reason codes. These meanings
must not be combined by callers.

The packaged golden pairs under
`fixtures/compatibility/request-response` are the authoritative examples for
all four categories. Callers should consume those fixtures or the evaluator,
not duplicate the rules.
