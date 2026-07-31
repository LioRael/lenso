# Internal Service Provider gRPC Transport

This note documents the legacy internal transport implementation used when a
Provider-mode Service exports Modules over gRPC. `Remote` names below are wire
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
/lenso.remote.v1.RemoteModule/GetManifest
/lenso.remote.v1.RemoteModule/ListAdminRecords
/lenso.remote.v1.RemoteModule/GetAdminRecord
/lenso.remote.v1.RemoteModule/InvokeAdminAction
/lenso.remote.v1.RemoteModule/ProxyHttpRoute
/lenso.remote.v1.RemoteModule/InvokeFunction
/lenso.remote.v1.RemoteModule/HandleEvent
```

The first implementation uses protobuf unary calls with one JSON payload field.
This keeps the existing `ModuleManifest`, runtime invoke, and event-handler
envelopes stable while proving the transport boundary. `ProxyHttpRoute` carries
the same host-owned HTTP proxy request/response envelopes used by the
HTTP/JSON lane, so auth, capability checks, header policy, and telemetry remain
host responsibilities. A later public protocol can replace the JSON field with
typed protobuf messages when the surface is ready to freeze. The checked-in
protocol file lives at
`contracts/grpc/lenso/remote/v1/remote_module.proto`.

## Deferred

- Custom CA and client certificate configuration.
- Streaming, bidirectional channels, and arbitrary host bridges.
