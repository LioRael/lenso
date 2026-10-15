# Optional TS Runtime adapter

Read `packages/runtime/README.md`, its public export map and `examples/runtime` from the selected source/artifact. `@lenso/runtime` integrates Core; `@lenso/runtime-sdk` owns transport. Local functions need neither package.

`createRuntimePlugin({ id, connection, clients, exports })` connects registered/unready during setup, registers drain/close immediately, then builds outbound clients. Declare that exact plugin in `requires` and use `context.get(runtime).clients`. For host-only use, supply `clients: () => undefined`.

A serving App uses `startRuntimeApp`. Its post-setup `exports(running, clients)` factory gets already initialized native plugins through `running.get(nativePlugin)`, allowing outbound client injection without a self-export dependency cycle. Select explicit handlers and wire contract; unselected methods stay local. The older `handlers(context)` setup factory remains an alternative with declared dependencies, never alongside `exports`.

The starter activates after successful setup/binding; failure rolls back and preserves cleanup failures. Stop invokes the registered Runtime disposers before Core resource cleanup, then Core's normal LIFO sees those same idempotent disposers. Plain Core `startApp` leaves the connection registered-only and can serve an outbound-only CLI app. `activateRuntimeApp` borrows app ownership; its caller must handle failure cleanup and pre-drain.

Current standalone Runtime `765db7a` supports TCP, stdio and WebSocket with cancellation/backpressure; the TS repo's pinned SDK snapshot can be older. Inspect the actual `connection` SDK type before choosing transport. Source preparation with a newer candidate may need dependency/lock integration by its owner; never bypass a frozen-install failure or change the root lock just to make a skill example pass.

Cross-service schema generation belongs to Runtime. Source-first authoring/Engine integration is separately in progress at this skill's baseline; JSON may be generated, but do not invent unverified authoring exports or require wire contracts for native services. Optional read-only Runtime Manage/Console views require explicit operations plus trusted current Auth evidence; registration alone exposes nothing.
