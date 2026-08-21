# ADR 0058: Select JSON-RPC over loopback HTTP for Bun request dispatch

- Status: accepted
- Date: 2026-08-21
- Extends: ADR 0035, ADR 0047, ADR 0048, ADR 0049, ADR 0050, ADR 0052,
  ADR 0053, ADR 0056
- Implements: #587, #588, #589, #590

## Context

The first Bun child-process Adapter needs one request wire without moving
process topology, framing, JSON codecs, or supervision into the portable
Kernel. The two candidates were:

1. Adapter-private length-prefixed JSON frames over Bun stdin/stdout.
2. JSON-RPC 2.0 over a loopback HTTP endpoint served by Bun.

Both candidates must carry the same generated Capability identity, exact
Descriptor version, exact Operation table, portable JSON value profile,
request correlation, success/Domain Error/Runtime Failure outcomes, deadlines,
cancellation, bounded admission, malformed-message rejection, and isolated
child-process failure. They must also be exercised by the same machine-readable
request corpus before transport-specific performance is considered.

The Rust Adapter uses the maintained [jsonrpsee HTTP client
stack](https://github.com/paritytech/jsonrpsee) for JSON-RPC request,
response, timeout, and body-limit handling. The Bun fixture uses the standard [Bun HTTP server
API](https://bun.com/docs/runtime/http/server) on `127.0.0.1`; no browser or
WASI capability is implied by the loopback transport.

## Decision

Select JSON-RPC 2.0 over loopback HTTP as the first supported Bun wire.

`BunAdapter::production` selects `BunWire::JsonRpcHttp`. The Adapter owns the
HTTP client, bounded worker and cancellation queues, exact handshake, request
codec bridge, body limits, response correlation, and child-process lifecycle.
The portable Kernel sees only its existing `ExecutionAdapter`,
`NativeRequestEndpoint`, lifecycle, stable-handle, deadline, and cancellation
interfaces.

The supported JSON-RPC methods are:

- `lenso.handshake`: exact protocol/value/endpoint admission before calls.
- `lenso.request`: one generated request and one tagged outcome.
- `lenso.cancel`: cooperative remote cancellation signal paired with local
  Kernel cancellation and no replay; the provider must observe the bounded
  cancellation token before returning its terminal outcome.

The Rust-side `BunProviderServer` is the reverse-direction Adapter seam for a
Bun consumer calling a Rust provider. It applies the same handshake, body
limit, bounded queue, exact endpoint validation, and wire failure translation.

Framed stdio remains in the Adapter as a reproducible prototype and benchmark
candidate (`BunWire::FramedStdio`). It is not the production default and is not
another Kernel protocol. Keeping the implementation and fixture makes the
selection reproducible while keeping the losing candidate out of the
supported product path.

## Selection criteria

The decision is bounded to one request/response Capability seam:

- **Dependency and complexity:** JSON-RPC adds the maintained Rust HTTP client
  dependency and loopback HTTP lifecycle; in exchange it removes the custom
  HTTP client/parser from the selected path and gives the reverse direction the
  same request vocabulary.
- **Host portability:** both candidates remain behind the native Bun
  Execution Adapter. Neither adds process, network, browser, or WASI concerns
  to the portable Kernel; Browser and WASI hosts do not gain this Adapter by
  accident.
- **Evolution:** protocol version, value profile, maximum body/frame size, and
  exact endpoint descriptors are admitted together. A mismatch is a bounded
  `ProtocolViolation`, not runtime discovery or fallback.
- **Scope:** streaming, events, subscriptions, and remote deployment are not
  hidden in this prototype. They require a later ADR with explicit Kernel and
  Adapter ownership.

## Protocol invariants

Every candidate uses:

- protocol version `1` and value profile `lenso-json-value-v1`;
- a maximum encoded frame/body size of 64 KiB by default;
- a bounded Adapter-owned request queue of 32 by default;
- exact `(Capability ID, Descriptor version, Operation table)` handshake;
- monotonically correlated request IDs, typed success and Domain Error values,
  and a tagged Runtime Failure value;
- rejection of malformed, unknown, duplicate, late, or out-of-order responses;
- no replay after child-process exit; the current stable handle becomes
  unavailable until the Kernel installs a fresh generation.

Streaming, events, subscriptions, schema negotiation beyond the exact
handshake, and remote/network deployment remain later decisions.

The comparison used the following explicit decision matrix:

| Criterion | Framed stdio | JSON-RPC over HTTP |
| --- | --- | --- |
| Request correctness | Passes the shared request and boundary corpus | Passes the same corpus in both call directions |
| Startup, latency, throughput, memory | Measured in the checked-in snapshot | Measured in the same process and hardware run |
| Cancellation and crash handling | Measured cancellation, crash detection, and clean respawn | Measured cancellation, crash detection, and clean respawn |
| Bounded overload | Provider rejects excess work as `ResourceExhausted` | Provider and reverse bridge reject excess work as `ResourceExhausted` |
| Dependency and debugging | No protocol dependency, but custom framing and diagnostics | Maintained `jsonrpsee`; ordinary HTTP/JSON-RPC tooling and errors |
| Browser and WASI portability | Native child-process Adapter only | Native child-process Adapter only; loopback HTTP does not imply browser or WASI support |
| Schema evolution | Exact version/profile/endpoint handshake | Same exact handshake plus standard request/error envelopes |
| Stream and Event readiness | No hidden support; requires a later ADR | Subscriptions are deliberately not selected; requires the same later ADR |

Correctness and bounded behavior are blockers. With parity established, the
selected path favors maintained framing, reverse-direction reuse, and standard
debug tooling over the prototype's lower raw overhead.

## Evidence

The shared request corpus is
[`fixtures/bun/request-conformance.json`](../../fixtures/bun/request-conformance.json).
The cross-runtime harness is
[`crates/lenso-bun-adapter/tests/bun_cross_runtime.rs`](../../crates/lenso-bun-adapter/tests/bun_cross_runtime.rs),
and the reproducible wire benchmark is
[`fixtures/bun/wire-benchmark.ts`](../../fixtures/bun/wire-benchmark.ts).
The checked-in local measurement snapshot is
[`docs/evidence/bun-wire-benchmark.json`](../evidence/bun-wire-benchmark.json).

Run a fresh comparison with:

```sh
LENSO_BUN_BENCHMARK_REQUESTS=50 \
  bun run fixtures/bun/wire-benchmark.ts -- \
  --output docs/evidence/bun-wire-benchmark.json
```

The benchmark records startup, p50/p95/p99 latency, sequential throughput,
maximum resident memory, cancellation latency, corpus outcome parity,
deadline/cancellation/size observations, child-process crash detection and
clean respawn latency, and directly observed bounded-overload outcomes for both
candidates. Its numbers are local diagnostic evidence, not a universal
performance claim; correctness parity and bounded behavior are selection
blockers.

## Consequences

JSON-RPC over loopback HTTP has a mature request/response vocabulary and makes
the reverse Bun-consumer direction explicit, at the cost of an HTTP server and
loopback request lifecycle. The implementation is still private to the
Adapter, so changing the wire cannot change the Kernel contract. Framed stdio
retains lower-level diagnostic value and remains available for future evidence
or a separately accepted decision, but production code must opt into it
explicitly.
