# lenso-contract-runtime

`lenso-contract-runtime` is the small platform-neutral support surface shared by
Rust bindings generated with `lenso-contract-codegen`.

It owns portable wire primitives, canonical Base64 bytes, validated `RawJson`
and typed `Json<T>` string payloads, missing-versus-null serde helpers,
forward-compatible unknown Domain Errors, and portable JSON number validation.
It does not own Capability-specific values, Provider traits,
Endpoint dispatch, Clients, Kernel semantics, async runtimes, networking, or OS
integration.

Patch and minor releases preserve wire behavior. Changes to that behavior require
an explicit compatibility decision and coordinated generated-artifact and
cross-language conformance updates.
