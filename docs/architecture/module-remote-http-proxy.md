# Remote Module HTTP Proxy

This note specifies the protocol boundary for exposing remote module-owned HTTP
routes through the host API. The current implementation preserves
`ModuleManifest::http_routes` as metadata and forwards matched GET requests
through a host-owned route. Non-GET methods, request bodies, and streaming
remain deferred.

## Current State

Remote modules can declare module-local routes in their manifest:

```json
{
  "http_routes": [
    {
      "method": "GET",
      "path": "/contacts",
      "capability": "remote_crm.contacts.read"
    },
    {
      "method": "GET",
      "path": "/contacts/{id}",
      "capability": "remote_crm.contacts.read"
    }
  ]
}
```

The host validates that remote route paths are module-local. They must start
with `/`, must not be absolute URLs, and must not contain empty, `.`, `..`,
query, or fragment segments. Valid declarations are exposed as metadata through
`/admin/data/modules`.

A `GET` public API route is installed at `/modules/{module}/http/{*path}`. It
matches the declaration, enforces service/system auth plus route capability, and
forwards the request to the remote module without caller credentials. If the
remote source has a configured auth token, the host uses that token for the
remote request. Successful remote responses must be JSON
(`application/json` or `application/*+json`) and must not exceed the current
4 MiB proxy response limit. Non-GET host methods are not mounted yet.

## Goals

- Allow a configured remote module to expose narrow HTTP endpoints through the
  host API without becoming an in-process Axum dependency.
- Keep the module manifest as pure serializable data.
- Make the host responsible for auth, capability enforcement, request limits,
  header policy, error normalization, and observability.
- Avoid implying OpenAPI coverage for dynamic remote routes until the host has a
  contract strategy.

## Non-goals

- Remote modules contributing Rust handlers or `OpenApiRouter`s.
- Runtime function execution.
- Event handler registration or remote outbox dispatch.
- Browser-facing embedded admin bridges.
- Arbitrary streaming, websocket, SSE, or multipart proxying.
- Marketplace trust, signatures, and install-time provenance.

## Host Namespace

Remote routes should be mounted under a host-owned namespace:

```text
/modules/{module}/http/{*path}
```

The module name is the configured module name, not a value supplied by the
remote process. The trailing path is matched against the declared
`ModuleHttpRoute::path` entries in the loaded manifest.

The host should not mount remote routes at `/v1/*`, `/admin/*`, or any
module-chosen absolute path. This prevents remote modules from shadowing core
API routes or other modules.

## Route Matching

The host should match by:

1. Configured module name.
2. HTTP method.
3. Declarative path pattern.

Supported path pattern syntax should initially be limited to:

- Literal segments, such as `/contacts`.
- Single path parameters, such as `/contacts/{id}`.

Catchalls, regexes, optional segments, matrix params, query params in the route
pattern, and duplicate parameter names should be rejected.

## Request Policy

The first proxy slices support only GET requests with JSON responses. Request
bodies and write methods are deferred.

Request constraints:

- Maximum request body size: host-configured, default 1 MiB.
- Maximum response body size: host-configured, default 4 MiB.
- Methods: GET only until request body policy is implemented.
- Content types: `application/json` and empty body only.
- Timeouts: use the remote module source timeout unless a narrower proxy
  timeout is configured.

Headers forwarded to the remote module should be allowlisted:

- `accept`
- `x-request-id`
- `x-correlation-id`
- `traceparent`

Future body-bearing methods may also forward `content-type` when JSON request
body policy is implemented.

Headers not forwarded:

- `authorization`
- `cookie`
- `set-cookie`
- `x-forwarded-*`
- hop-by-hop headers such as `connection`, `upgrade`, `te`,
  `transfer-encoding`, and `keep-alive`

The host may authenticate to the remote module using the configured remote
module token, but it must not forward the caller's bearer token.

## Auth And Capabilities

The host owns caller authentication. Remote routes must require service/system
auth by default until user-facing policy exists.

If a route declares `capability`, the host must enforce that capability before
proxying. If no capability is declared, the route should be treated as blocked
for external callers unless the host has an explicit allow policy for that
module.

The remote module can still perform its own authorization, but that is defense
in depth. Host enforcement is required because the route is exposed under the
host API.

## Error Mapping

Remote modules should return the standard platform error envelope:

```json
{
  "error": {
    "code": "not_found",
    "message": "contact contact_404 was not found",
    "retryable": false,
    "details": []
  }
}
```

The host should normalize errors before returning them:

- Valid remote error envelopes map to host `AppError`s.
- Remote 5xx responses map to `external_dependency_failure`.
- Remote 429 maps to `rate_limited`.
- Remote transport, timeout, invalid JSON, and response body limit failures map
  to `external_dependency_failure`.
- Host auth/capability failures are generated by the host and are not proxied.

The host should add diagnostic details such as `remote_status`, `remote_code`,
and `remote_module`, while preserving the public request correlation context.

## Observability

Each proxied call should produce host-side telemetry:

- Module name.
- Declared route path.
- Actual host path.
- HTTP method.
- Remote status.
- Duration.
- Retryability.
- Error code when present.

Remote module response headers should not be used as trusted telemetry unless
explicitly allowlisted.

## OpenAPI Strategy

Dynamic remote routes should not be added to the static committed OpenAPI
document by default. The committed OpenAPI artifact is generated from Rust
handlers and must remain context-free.

Initial implementation should expose one static proxy route shape:

```text
/modules/{module}/http/{*path}
```

That route can document the proxy envelope and limitations, but not every
module-owned endpoint. A later install-time contract system may expose per-module
OpenAPI fragments after trust, validation, and versioning are specified.

## Implementation Order

1. Add a host proxy registry from loaded remote module manifests. Done.
2. Add route matching for method plus simple path patterns. Done.
3. Add one static host proxy route under `/modules/{module}/http/{*path}`. Done
   for GET.
4. Enforce service/system auth and declared capabilities. Done for GET.
5. Forward matched GET requests without caller credentials. Done; configured
   host-to-remote bearer tokens are used when present.
6. Add request/response size limits and full header allowlists. Done for GET
   response content-type, response size, and header allowlists; request bodies
   remain deferred because only GET is mounted.
7. Mount the remaining declared methods: `POST`, `PUT`, `PATCH`, and `DELETE`.
8. Normalize remote errors through the existing platform error model. Done for
   GET.
9. Add telemetry and runtime-console visibility for proxied calls.

Do not implement per-module OpenAPI fragments, streaming, browser credentials,
or bidirectional admin bridges in the first proxy slice.
