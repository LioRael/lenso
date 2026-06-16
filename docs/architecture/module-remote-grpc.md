# Remote Module gRPC Transport

This note defines the first native gRPC transport lane for remote modules. It
does not replace the existing HTTP/JSON protocol. The host still owns auth,
timeouts, retries, outbox claims, Runtime Story semantics, and operator
visibility.

## Current Lane

Configure a gRPC remote module by using a `grpc://` endpoint in `REMOTE_MODULES`:

```text
REMOTE_MODULES=remote-crm=grpc://127.0.0.1:50051
```

The host normalizes that endpoint to a tonic `http://` channel and calls:

```text
/lenso.remote.v1.RemoteModule/GetManifest
/lenso.remote.v1.RemoteModule/ListAdminRecords
/lenso.remote.v1.RemoteModule/GetAdminRecord
/lenso.remote.v1.RemoteModule/InvokeAdminAction
/lenso.remote.v1.RemoteModule/InvokeFunction
/lenso.remote.v1.RemoteModule/HandleEvent
```

The first implementation uses protobuf unary calls with one JSON payload field.
This keeps the existing `ModuleManifest`, runtime invoke, and event-handler
envelopes stable while proving the transport boundary. A later public protocol
can replace the JSON field with typed protobuf messages when the surface is
ready to freeze. The checked-in protocol file lives at
`contracts/grpc/lenso/remote/v1/remote_module.proto`.

## Deferred

- gRPC-backed public HTTP proxy routes.
- TLS configuration for `grpcs://`.
- Streaming, bidirectional channels, and arbitrary host bridges.
