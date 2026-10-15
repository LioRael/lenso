# Runtime role and source-first direction

Runtime owns kernel/protocol, SDKs, transport and wire generation. Native framework repos own plugin composition and adapters; Console owns its optional presentation. Keep native calls within a service; only selected cross-service capabilities need serializable contracts. A kernel is not a replacement for TS Lenso, a configuration center, an authentication server or an application container.

At `765db7a`, TCP, persistent stdio and trusted local WebSocket are implemented with cancellation and stream credit/backpressure. Do not reduce the architecture to TCP or create one process per request. Native Rust's older pinned adapter has narrower configuration than the current SDK; check the actual layer.

The currently verified generator consumes a restricted JSON Schema contract and emits typed TS/Rust bindings. A source-first authoring layer may produce that JSON; this is the desired direction, not permission to invent an API or require contracts for local functions. Record source-first work as pending/candidate until the selected repository includes its exports, Engine wiring and a passing example. Retain existing JSON contracts as valid explicit escape hatches.

Readiness follows native setup and explicit binding. Drain stops admission before resources close. Transport context is explicit; timeout/cancellation is not rollback, delivery certainty or safe retry. Keep storage handles/transactions/actors local and pass bounded values or authorized references. Service identity is connection-bound trusted-local process identity, not end-user proof.

Use matching source/SDK/binary proofs for candidate integration. Keep builders, lock ownership and generated output with their repository owner. A native app change needs a focused native proof; a wire change needs a real generated cross-language call. Performance claims require their own requested experiment and cannot come from a successful smoke.
