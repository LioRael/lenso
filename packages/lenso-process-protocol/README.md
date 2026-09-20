# @lenso/process-protocol

Runtime-neutral TypeScript types, strict JSON decoding, proof framing, and
conformance helpers for `lenso-process-jsonrpc-http-v1`. The package also
exports transport-neutral Authoring V2 values and validators; Process and Bun
adapters supply their own envelopes.

Execution Adapters and child SDKs use this package at their wire boundary. It
does not spawn processes, open HTTP listeners, generate random secrets, perform
HMAC, interpret Capability payloads, or own Plugin behavior.

## Execution target capability profiles

`execution-target-capability-profile-v1.schema.json` and the exported
`ExecutionTargetCapabilityProfile` helpers define a versioned, closed
declaration for one exact Adapter or Driver runtime profile. The supported V1
feature names are `request`, `stream`, `event`, `websocket`, `host-imports`,
`native-process`, `wasm-component`, `remote`, `browser`, and `workers`.

Profiles are explicit: a `target_profile` name grants no inferred behavior.
Hosts compare the declared `capabilities` against the selected implementation's
needs before dispatch. Unknown features, missing capability lists, invalid
profiles, and undeclared requirements fail closed. The capability list is
canonical-sorted and unique; an empty list intentionally declares no V1
features.

The profile contract describes target behavior only. It does not select a
Plugin provider, change an immutable Resolved App Plan, or decide Host policy.

The published `schemas/process-protocol-v1.schema.json` describes every V1
request and successful-response envelope. Runtime validators additionally
enforce constraints JSON Schema cannot express, including duplicate-key
rejection, canonical array ordering, and expected identity/session equality.
