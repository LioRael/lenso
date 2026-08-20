# Define a portable JSON value contract

Portable Capability Descriptors will reference package-local JSON Schema
2020-12 documents and apply a small Lenso value profile where Rust and
JavaScript otherwise disagree. Build tooling resolves and bundles every `$ref`;
Kernel and Execution Adapters never fetch schema documents at runtime.

## Consequences

- Ordinary JSON integers are limited to the JavaScript safe-integer range.
  Wider signed and unsigned integers use explicit `int64` and `uint64` formats
  represented as decimal strings on the wire. Bytes use an explicit base64
  format, and timestamps and durations use documented portable formats.
- Missing and `null` remain distinct. JSON Schema `required` controls presence,
  and a value permits `null` only when its schema says so. Rust and TypeScript
  generators preserve that distinction.
- Cross-process Adapters validate decoded wire values at their input boundary.
  Native typed dispatch does not repeat JSON Schema validation on every call;
  debug and conformance configurations may enable additional checks.
- Operation names are stable identities within one `namespace.name@major`
  series and are never reused for another meaning. Array position or source
  declaration order has no protocol significance.
- Each Operation defines an open tagged union of Domain Errors with stable
  codes and structured payload schemas. Older generated clients preserve an
  unknown code and payload as `UnknownDomainError`; Runtime Failures remain a
  separate channel.
- Patch releases do not change observable contract behavior. Minor releases may
  add Operations, optional fields, and open variants. Removing, renaming,
  narrowing, changing a type or meaning, or changing an interaction kind
  requires a new major series. Tooling lints these rules; Kernel does not diff
  schemas or infer compatibility.
