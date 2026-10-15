# Diagnose the actual Runtime layer

Identify native TS App adapter, standalone SDK, kernel and the selected peer/source provenance. A matching version string does not establish matching SDK/binary bytes. Read `packages/runtime/README.md` and the selected Runtime README/PROTOCOL section; use the Runtime skill if available for wire-level work.

- Registered is not ready. Confirm successful Core setup, explicit export binding and activation (`startRuntimeApp` for a serving TS App). Plain `startApp` intentionally leaves an outbound connection unready. Do not expose additional methods as a repair.
- TCP, persistent stdio and local WebSocket are implemented in current Runtime source; inspect the consumer's pinned SDK before choosing options. In stdio, any non-protocol stdout before ready or during cleanup is a framing defect; business logs use stderr.
- Distinguish unavailable service, unready, overload, deadline, connection loss, invalid contract/data and declared business failure. Inspect safe existing diagnostics; don't replay an uncertain write or invent a generic management CLI.
- Cancelled correlation does not mean a JS handler/iterator finished. A handler ignoring abort retains local execution occupancy; drain waits within its bound. Streams need awaited credit/backpressure through the SDK, not an unbounded producer queue.
- SDK source, generated bindings and native binaries must match the selected contract/provenance. A local-candidate dependency change can make frozen install fail; report that integration gap rather than rewriting the lock or claiming the pinned artifact supports the new feature.

Use one existing focused smoke for a changed wire boundary. Native Core diagnosis alone needs no kernel/transport matrix. Close only task-owned processes/ports and report whether evidence came from SDK-only or actual native App integration.
