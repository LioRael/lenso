# Native frameworks

Rust source baseline is `e5dfafb`, containing Core, Log, bounded Cache, native log-source examples and the optional Runtime adapter. Use the `lenso-rust` skill for actual APIs. Rust services are `Arc<S>` with exact `Handle<S>` dependencies. Shared ownership does not imply a thread-safe business algorithm or async Drop.

Outer Rust business entries admit once through WorkGate; dependencies such as logging/cache reuse that admitted work. Startup is owned and cancellation-safe at this baseline; shutdown seals admission, stops entrances, drains work and releases resources. Do not preserve obsolete `f10badf` startup assumptions or copy TS global LIFO as if it were Rust's phased cleanup.

Go source is a local prototype under module `lenso.local/go`, not a published import path. It uses `New`, `Definition[T]`, `Ref[T]`, `Resolve`, `Start`, `Get`, `Close` with ordinary Go methods/interfaces. Keep its skill/evidence separate until a versioned module and Runtime SDK delivery exist. Context cancellation is cooperative; Close waiter cancellation does not stop cleanup. It has no Rust WorkGate or cleanup phase API.

Prefer native configuration structs and functions. Do not translate TS Standard Schema/Engine/Pino options by spelling. A library can be owned by a plugin directly without a framework wrapper for every method. Only share a native crate/package when an actual second user or release boundary needs it.

Limits, Authorization, Audit, Storage and Realtime Rust implementations are parallel work, absent from the pinned baseline. Do not describe them as shipped because their task directories exist. Integrate guidance only after checking the exported API and its executable example on the chosen commit. Explicitly document unsupported provider/durability behavior; process memory is not persistent storage.
