# Contract compatibility

The public compatibility categories are `safe`, `needs_attention`, `breaking`, and `blocked`.
Every result identifies its contract kind, changed version, affected references, stable reason
codes, JSON paths, and next actions. Provider Protocol, Service Contract, Event Contract, Config
Contract, and Reliability Contract are distinct machine-readable kinds.

Event comparison is independent of broker choice and evaluates canonical JSON Schema or Protobuf
business-event shapes. Generated Event Contract comparison additionally covers protocol, stable
Producer and Module identity, Tenancy Mode, required common context, Operating Regions, and the
embedded authoritative payload schema. Config comparison covers required values, sensitivity,
scope, mutability, and activation requirements. Reliability comparison is conservative:
declaration changes require review and never claim that Lenso enforces availability, latency,
health, backlog, error-budget, or rollout expectations at runtime.

The Event, Config, and Reliability golden pairs under `fixtures/compatibility` are authoritative
deterministic examples. Their exact sorted machine results are generated into
`contracts/compatibility/contract-compatibility.v1.json` and freshness-checked by `arch-check`.
