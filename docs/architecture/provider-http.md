# Provider Service HTTP Boundary

This note specifies the protocol boundary for exposing Service-delivered Module HTTP
routes through the host API. The current implementation preserves
`ModuleManifest::http_routes` as metadata and forwards matched GET, POST, PUT,
PATCH, and DELETE requests through a host-owned route. Streaming remains
deferred.

## Current State

Provider exports can declare Module-local routes in their locked manifest:

```json
{
  "http_routes": [
    {
      "method": "GET",
      "path": "/contacts",
      "capability": "provider_crm.contacts.read"
    },
    {
      "method": "GET",
      "path": "/contacts/{id}",
      "capability": "provider_crm.contacts.read"
    }
  ]
}
```

The host validates that provider route paths are module-local. They must start
with `/`, must not be absolute URLs, and must not contain empty, `.`, `..`,
query, or fragment segments. Valid declarations are exposed as metadata through
`/admin/data/modules`.

`GET`, `POST`, `PUT`, `PATCH`, and `DELETE` public API routes are installed at
`/modules/{module}/http/{*path}`. They match the declaration, enforce
service/system auth plus route capability, and forward the request to the provider
export without caller credentials. HTTP/JSON Provider transports receive ordinary
HTTP requests; gRPC provider sources receive the same request data through
`/lenso.provider.v1.Provider/ProxyHttpRoute`. If the provider source has a
configured auth token, the host uses that token for the provider request.
Successful provider responses must be JSON (`application/json` or
`application/*+json`) and must not exceed the current 4 MiB proxy response
limit. POST, PUT, and PATCH request bodies must be JSON and must not exceed the
current 1 MiB proxy request limit. DELETE request bodies must be empty; DELETE
`204 No Content` success responses are returned in the proxy envelope with
`data = null`.

## Goals

- Allow a configured Provider Service to expose narrow HTTP endpoints through the
  host API without becoming an in-process Axum dependency.
- Keep the module manifest as pure serializable data.
- Make the host responsible for auth, capability enforcement, request limits,
  header policy, error normalization, and observability.
- Avoid implying OpenAPI coverage for dynamic provider routes until the host has a
  contract strategy.

## Non-goals

- Provider exports contributing Rust handlers or `OpenApiRouter`s.
- Runtime function execution.
- Event handler registration or provider outbox dispatch.
- Browser-facing embedded admin bridges.
- Arbitrary streaming, websocket, SSE, or multipart proxying.
- Operator trust decisions for explicit module installs and official catalog
  curation.

## Host Namespace

Provider routes should be mounted under a host-owned namespace:

```text
/modules/{module}/http/{*path}
```

The module name is the configured module name, not a value supplied by the
provider process. The trailing path is matched against the declared
`ModuleHttpRoute::path` entries in the loaded manifest.

The host should not mount provider routes at `/v1/*`, `/admin/*`, or any
module-chosen absolute path. This prevents Provider Services from shadowing core
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

The mounted proxy slices support GET requests with JSON responses, POST, PUT,
and PATCH requests with JSON request/response bodies, and DELETE requests with
empty request bodies plus JSON or empty success responses.

Request constraints:

- Maximum request body size: host-configured, default 1 MiB.
- Maximum response body size: host-configured, default 4 MiB.
- Methods: GET, POST, PUT, PATCH, and DELETE.
- Content types: GET responses must be JSON; POST, PUT, and PATCH request and
  response bodies must be JSON; DELETE success responses must be JSON unless
  the provider returns `204 No Content` with an empty body.
- Timeouts: use the Provider Service timeout unless a narrower proxy
  timeout is configured.

Headers forwarded to the Provider Service should be allowlisted:

- `accept`
- `x-request-id`
- `x-correlation-id`
- `traceparent`

POST, PUT, and PATCH forward `content-type` only after the JSON request body
policy accepts the request. Future body-bearing methods should use the same
policy.

Headers not forwarded:

- `authorization`
- `cookie`
- `set-cookie`
- `x-forwarded-*`
- hop-by-hop headers such as `connection`, `upgrade`, `te`,
  `transfer-encoding`, and `keep-alive`

The host may authenticate to the Provider Service using the configured provider
module token, but it must not forward the caller's bearer token.

## DELETE Policy

DELETE uses the same host namespace, route matching, service/system auth,
capability enforcement, header allowlist, tracing, and configured host-to-provider
bearer token behavior as the other proxy methods.

DELETE request bodies are not accepted in the first implementation. The host
forwards an empty DELETE request to the Provider Service and rejects any caller
request with a non-empty body as `validation_failed`. This keeps DELETE
semantics narrow and avoids introducing a second body policy for methods where
payload support is often ambiguous across clients, caches, and intermediaries.

DELETE responses should support both JSON and empty success responses:

- `200 OK` with JSON response body: decode the JSON and return the normal
  `ProviderHttpProxyResponse` envelope with `status = "forwarded"` and `data`
  set to the provider JSON.
- `202 Accepted` with JSON response body: same as `200 OK`; the Provider Service
  can use the JSON body to report asynchronous deletion state.
- `204 No Content` with an empty body: return the normal
  `ProviderHttpProxyResponse` envelope with `status = "forwarded"` and
  `data = null`.

DELETE success responses with a non-empty non-JSON body should be rejected as
`external_dependency_failure`, matching the existing response content-type
policy. DELETE error responses should continue to use the standard provider error
envelope when possible, and fallback status mapping otherwise.

## Auth And Capabilities

The host owns caller authentication. Provider routes must require service/system
auth by default until user-facing policy exists.

If a route declares `capability`, the host must enforce that capability before
proxying. If no capability is declared, the route should be treated as blocked
for external callers unless the host has an explicit allow policy for that
module.

The Provider Service can still perform its own authorization, but that is defense
in depth. Host enforcement is required because the route is exposed under the
host API.

## Error Mapping

Provider exports should return the standard platform error envelope:

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

- Valid provider error envelopes map to host `AppError`s.
- Provider 5xx responses map to `external_dependency_failure`.
- Provider 429 maps to `rate_limited`.
- Provider transport, timeout, invalid JSON, and response body limit failures map
  to `external_dependency_failure`.
- Host auth/capability failures are generated by the host and are not proxied.

The host should add diagnostic details such as `provider_status`, `provider_code`,
and `provider`, while preserving the public request correlation context.

## Observability

Each proxied call should produce host-side telemetry:

- Module name.
- Declared route path.
- Actual host path.
- HTTP method.
- Provider status.
- Duration.
- Retryability.
- Error code when present.

Provider export response headers should not be used as trusted telemetry unless
explicitly allowlisted.

Current GET, POST, PUT, PATCH, and DELETE proxy calls emit structured host-side
tracing events for completed and failed forwards with module name, declared
path, provider path, method, provider status, duration, request/correlation ids, and
error code/retryability when present. Calls are also persisted to
`platform.provider_http_calls` with module, route, status, duration,
request/correlation, trace/span, path parameter, and error detail fields.

The Host also persists a bounded evidence copy of Provider route request and
response JSON bodies. This evidence copy has a separate 64 KiB per-body limit;
the existing 1 MiB request and 4 MiB response transport limits remain unchanged.
Before the storage Interface receives a body, the Provider adapter recursively
replaces values whose keys contain authorization, cookie, password, secret,
token, API key, access key, credential, or email markers with `[redacted]`.
Caller and Provider headers are never part of the body evidence record.

Every side records `captured`, `not_applicable`, or `not_captured` together with
an explicit reason and the observed serialized size when available. Bodies over
the evidence limit are not partially copied; the record keeps only
`not_captured`, `evidence_limit_exceeded`, and the observed size. Existing rows
are migrated as `not_captured` with `legacy_record`, while methods without a
request body and empty successful responses use `not_applicable` reasons. Raw
bodies must never be written to Story event metadata; that compact metadata may
contain only the capture status, reason, and size needed to explain evidence
coverage.

Console exposes persisted proxy calls through three surfaces:

- `/operations/provider-calls` is the horizontal operational view for filtering
  across stories by module, success, error code, provider status, and correlation
  id. A selected call or correlation filter can open the matching Runtime
  Story.
- Runtime Story graph and timeline include proxy calls as ordinary
  `provider_proxy_call` nodes scoped to the selected story's `correlation_id`.
  The Story detail should not duplicate the same facts in a separate section.
  Story node `metadata.source_metadata` is the compact/detail UI contract and
  carries module, method, declared path, provider path/status, duration,
  request/trace/span ids, path params, error code, retryability, and error
  details.
- Runtime Story Technical Operations includes proxy calls as
  `source = "provider_proxy"` operations. A proxy call attaches to a story node
  when its `span_id` matches an OTEL span with `lenso.function_run_id` or
  `lenso.outbox_event_id`; if the span id is unavailable, trace id attributes
  provide a fallback. Unmatched calls remain story-level operations.

## OpenAPI Strategy

Dynamic provider routes should not be added to the static committed OpenAPI
document by default. The committed OpenAPI artifact is generated from Rust
handlers and must remain context-free.

Initial implementation should expose one static proxy route shape:

```text
/modules/{module}/http/{*path}
```

That route can document the proxy envelope and limitations, but not every
module-owned endpoint. A later contract system may expose per-module OpenAPI
fragments after validation and versioning are specified.

## Implementation Order

1. Add a host proxy registry from loaded Service-delivered Module manifests. Done.
2. Add route matching for method plus simple path patterns. Done.
3. Add one static host proxy route under `/modules/{module}/http/{*path}`. Done
   for GET, POST, PUT, PATCH, and DELETE.
4. Enforce service/system auth and declared capabilities. Done for GET, POST,
   PUT, PATCH, and DELETE.
5. Forward matched GET, POST, PUT, PATCH, and DELETE requests without caller
   credentials. Done; configured host-to-provider bearer tokens are used when
   present.
6. Add request/response size limits and full header allowlists. Done for GET
   response content-type, POST/PUT/PATCH request/response content-type, DELETE
   empty request bodies and JSON or empty success responses, request/response
   size limits, and header allowlists.
7. Mount the remaining declared methods: `POST`, `PUT`, `PATCH`, and `DELETE`.
   Done.
8. Normalize provider errors through the existing platform error model. Done for
   GET, POST, PUT, PATCH, and DELETE.
9. Add telemetry and Console Runtime Stories visibility for proxied calls. Done for GET,
   POST, PUT, PATCH, and DELETE tracing events, persisted call history, Provider
   Calls filtering, Story `provider_proxy_call` nodes, and Story Technical
   Operations.

Do not implement per-module OpenAPI fragments, streaming, browser credentials,
or bidirectional admin bridges in the first proxy slice.
