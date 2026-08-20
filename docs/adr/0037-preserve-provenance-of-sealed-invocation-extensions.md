# Preserve provenance of sealed invocation extensions

Invocation Context extensions will distinguish ordinary caller-supplied baggage from sealed values established by an explicitly bound issuer. The Kernel will not interpret a sealed value's Auth, telemetry, tenant, or other domain meaning, but it will preserve the issuer, intended audience, and non-overwrite property while routing the call across Runtime Adapters.

## Consequences

- Sealing protects provenance through supported Interfaces; it is not a sandbox against malicious native code. Native Rust and the initial Bun Modules are trusted, while untrusted execution remains deferred to a Wasm or sandboxed-process Adapter.
- A Module cannot forge or replace an Actor assertion or another protected extension merely by writing the same key.
- Sealing does not imply confidentiality. Protected extensions still minimize disclosed identity and claims and never carry raw credentials or session secrets.
- Domain Modules define extension payloads and validation; the Kernel enforces only generic provenance and propagation mechanics.
- Runtime Diagnostics exclude extension values by default.
- Caller Module identity remains separate Kernel-owned invocation metadata and cannot be supplied as baggage or a sealed extension.
