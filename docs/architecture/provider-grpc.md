# Internal Service Provider gRPC Transport

> **Legacy v0.3.x architecture:** This page describes the maintained
> Service-oriented implementation and is not normative for vNext. Read
> [lenso-vnext.md](lenso-vnext.md) for vNext decisions.

This note documents the legacy internal transport implementation used when a
Provider-mode Service exports Modules over gRPC. `Provider` names below are wire
and crate compatibility details, not a public Module delivery kind. The Host
still owns auth, timeouts, retries, outbox claims, Runtime Story semantics, and
operator visibility.

## Current Lane

Select `provider_grpc` in the target-owned Service Installation endpoint
binding. The Application Module Lock selects the exact exported Module; the
Provider Runtime Plan supplies the endpoint to this internal adapter:

```text
binding: provider_grpc
address: https://crm.example.test:50051
```

The host normalizes those endpoints to tonic `http://` or `https://` channels
after `HostComposition` resolves the plan's endpoint source. Static declarations
need no resolver; local-process and adapter declarations require an endpoint
resolver registered under the exact source ID. The same composition selects a
credential resolver by the identity policy's exact trust profile. Resolved
credentials stay in transport memory and are redacted from Debug output. It
then calls:

```text
/lenso.provider.v1.Provider/GetManifest
/lenso.provider.v1.Provider/ListAdminRecords
/lenso.provider.v1.Provider/GetAdminRecord
/lenso.provider.v1.Provider/InvokeAdminAction
/lenso.provider.v1.Provider/ProxyHttpRoute
/lenso.provider.v1.Provider/InvokeFunction
/lenso.provider.v1.Provider/HandleEvent
```

The first implementation uses protobuf unary calls with one JSON payload field.
This keeps the existing `ModuleManifest`, runtime invoke, and event-handler
envelopes stable while proving the transport boundary. `ProxyHttpRoute` carries
the same host-owned HTTP proxy request/response envelopes used by the
HTTP/JSON lane, so auth, capability checks, header policy, and telemetry remain
host responsibilities. A later public protocol can replace the JSON field with
typed protobuf messages when the surface is ready to freeze. The checked-in
protocol file lives at
`contracts/grpc/lenso/provider/v1/provider.proto`.

## Deferred

- Custom CA and client certificate configuration.
- Streaming, bidirectional channels, and arbitrary host bridges.
