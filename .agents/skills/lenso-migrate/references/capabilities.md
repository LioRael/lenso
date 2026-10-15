# Capability map and availability

Checked source baselines: TS `74139b0`, Rust `e5dfafb`, Runtime `765db7a`. Do not confuse those with published package versions. Confirm installed export maps/Cargo source before generating imports.

| Capability | TypeScript Lenso | Native Rust at baseline | Port decision |
| --- | --- | --- | --- |
| Core composition | `@lenso/core`: Plugin, definePlugin/defineApp/startApp | `lenso-plugin-core`: Plugin, Instance, Handle, App | Preserve exact instance graph; ordinary services remain native |
| Config | Plain factory options or Core config sources/Standard Schema | Plain data, Plugin::validate, optional serde Config<T> | Different merge/env/validation semantics; translate deliberately |
| Logging | `@lenso/log` | `lenso-log` | Semantic subset; tracing/stderr, not Pino options or an OTel exporter |
| Cache | `@lenso/cache` | `lenso-cache` | Bounded process-local typed cache; not a full TS provider/protocol port |
| Web / Workers | `@lenso/web`, `@lenso/workers` | No equivalent native framework host in baseline | Keep chosen native HTTP host at edge; do not invent lenso-web-rust |
| Auth / permissions | `@lenso/auth`, `@lenso/authorization` | No integrated equivalent | Auth proof cannot be serialized into authority; pending Authorization branch is not available here |
| DB / private objects / files | `@lenso/db`, `@lenso/storage` | No integrated Storage/DB adapter; JSON-lines sample only | Choose explicit native provider/owner; preserve durability and policy |
| CLI / Engine / Manage / MCP | Separate TS optional packages | No equivalent native package set | Keep existing TS management entry or a requested native adapter; don't duplicate business logic |
| Limits / Audit / Realtime | Existing TS packages | Separate pending native tasks | Verify delivered crate APIs and semantics before using; no parity claim |
| Scheduler / Tasks and other plugins | TS optional ecosystem | No equivalent declared here | An ordinary Rust method is not a durable job/lease system |
| Runtime | `@lenso/runtime` over `@lenso/runtime-sdk` | `lenso-plugin-runtime` over pinned `runtime-sdk` | Explicit optional export, distinct native/SDK version boundaries |

Rust log/cache semantics are narrower: Cache uses Duration and JSON roundtrips, independent scopes, owned backend, bounded loaders; Log uses structured tracing, explicit scopes and stderr. Read their crate READMEs if actually porting those features.

Go currently has local `lenso.local/go`: `Definition[T]`, `*Instance[T]`, `Ref[T]`, `Start`, `Resolve`, `Get`, `Close`. No published module/version or full ecosystem equivalence follows from that. Its evolving Runtime SDK must have its own delivery evidence.

Known older Rust checkpoint `f10badf` lacks current WorkGate, early cleanup handles and protected Core startup. Do not copy its cancellation warning onto `e5dfafb`; do not use the newer APIs against the old pin either.
