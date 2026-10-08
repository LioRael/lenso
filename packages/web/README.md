# @lenso/web

Optional Fetch/oRPC adapter for Lenso. The application owns the HTTP listener,
or can opt into the Bun-specific listener adapter at `@lenso/web/bun`.
`createWebPlugin({ requires, router, prefix?, errorStatusMap?, fetch?, timeoutMs?, maxChunkBytes?, onError? })`
exposes `WebService.fetch(request): Promise<Response>`. The default RPC prefix is `/rpc`.

`router(pluginContext)` builds an oRPC 2.0.0-beta.42 router. Optional
`fetch(pluginContext)` builds a raw handler receiving `WebContext`; returning
`undefined` falls through to oRPC. Raw responses keep their status, headers and
bytes without token-by-token RPC encoding. Use oRPC's `asyncIteratorObject`
when you actually need typed RPC events.

`createClient<Router>(endpoint, { fetch?, headers? })` from `@lenso/web/client`
keeps the full endpoint URL API, splitting it into oRPC's `origin` and path `url`.
Relative endpoints resolve against the browser location; server callers supply an
absolute URL. Custom Fetch functions receive a URL string and request options.
RPC calls use POST; GET/HEAD requests do not invoke procedures. `errorStatusMap`
extends the common HTTP status map for application error codes. Errors contain no
`status` field. Upgrade clients and servers together because the RPC wire format changed.

oRPC no longer deduplicates middleware applied at both router and procedure level.
Apply authentication middleware once, or explicitly guard repeated application
with a request-context flag. This adapter adds no implicit middleware or auth cache.

## Explicit HTTP/OpenAPI endpoints

Install `@orpc/openapi@2.0.0-beta.42` only when using `@lenso/web/openapi`.
It is an optional peer; Core, Engine, CLI, MCP and the ordinary `/rpc` entry do
not import it. `createOpenAPIAdapter` wraps the official `OpenAPIHandler`.
It does not create business handlers, listeners, Manage exposure or docs routes.

```ts
import { createOpenAPIAdapter } from "@lenso/web/openapi";
import { openapi } from "@orpc/openapi";
import { ORPCError, os } from "@orpc/server";
import { createWebPlugin, type WebContext } from "@lenso/web";

// Reuse the application's shared schema, service and authorization rules.
const read = os
  .$context<WebContext>()
  .meta(openapi({ method: "POST", path: "/read" }))
  .input(readInput)
  .output(readOutput)
  .handler(({ input, context }) => service.read(input, context));
const rpcRouter = { read, privateOperation };
const selectedRouter = { read }; // Same procedure object, explicit HTTP allowlist.
const api = createOpenAPIAdapter<WebContext>({
  selectedRouter,
  prefix: "/api",
  authenticate: async (request, context) => {
    if (!(await verifyRequest(request, context))) {
      throw new ORPCError("UNAUTHORIZED");
    }
  },
});
const web = createWebPlugin({
  requires: [],
  router: () => rpcRouter,
  fetch: () => api.fetch,
});
const spec = await api.generateSpec({
  info: { title: "Selected application API", version: "1.0.0" },
});
```

In plugin setup, bind these procedures to the same initialized dependencies as
RPC. `selectedRouter` is application-owned and must contain only deliberately
published, eager procedures. Omitted selection exposes **none**. Every selected
procedure requires explicit `openapi({ method, path })` metadata; missing routes
and lazy selections fail at construction. Prefix and `authenticate` are required.
Routes must use canonical absolute paths, with literal path segments or complete
`{name}` / `{+name}` parameter segments. Empty segments, trailing slashes (except
the root route), dot segments, percent encodings, query strings and fragments
are refused. Metadata prefixes use canonical literal paths. Supported methods
are `HEAD`, `GET`, `POST`, `PUT`, `DELETE`, `PATCH` and `QUERY`; explicit success
statuses must be safe integers from 200 through 399. Invalid metadata fails before
a handler or spec can serve it.
For a deliberately public endpoint supply `authenticate: () => {}` explicitly.
Admission runs before routing within that prefix. Application/service rules still
own object/tenant authorization; selection is not permission. Do not mutate the
selected router or its procedures after construction.

`handle(request, context)` returns `undefined` outside the selected prefix.
`fetch(context)` plugs into the Web raw Fetch hook and preserves its existing
request signal, stream ownership, cleanup and drain. Within the prefix, missing
paths produce an opaque 404 Problem Details response. No `/spec.json` or docs UI
is mounted: publishing `generateSpec` output requires a separate explicit policy.
Spec generation uses the exact same selection and prefix, with OpenAPI 3.2
`application/problem+json` error schemas and the configured actual statuses.
Schema defaults/examples are omitted. Schema names, descriptions and constraints
can still be public: never put credentials into schema metadata.

Zod 4's Standard JSON Schema support works without another converter dependency.
Other Standard Schema implementations must provide Standard JSON Schema or an
explicit `converters` entry. Unsupported or failed conversion throws instead of
emitting fabricated `{}` schemas. Converters are trusted application code and
must fail when conversion is unsupported. Query/path values remain strings:
coerce in the shared input schema when needed; this adapter adds no implicit
coercion or retry policy.

### Safe Problem Details

`@lenso/web/problem-details` exports `createProblemDetails` for other explicit
HTTP boundaries. Use `createProblemDetails(options).response(error)` for a Fetch
response with the same status and `application/problem+json` policy. The adapter
uses this helper too. Existing third-party protocols are not migrated automatically.
Actual `ORPCError` instances
with known common codes map to fixed public status/title/detail; arbitrary
objects with a `code` do not. Unknown errors and unknown codes always become
fixed `INTERNAL_SERVER_ERROR`/500. Raw error messages, data, causes, validation
paths and request URLs are never copied into these responses.

`codes: { DUPLICATE: { status: 409, title: "Duplicate", detail: "Choose another name." } }`
adds a trusted fixed domain definition. `mapError(error)` may return a code, but
that code must exist in the safe map. All configured statuses (including RPC's
`errorStatusMap`) must be safe integers from 400 through 599; fixed problem
titles/details are bounded to 512 characters. The opaque internal fallback cannot
be overridden. No arbitrary extensions or inferred `Retry-After` are emitted.

Type identity is stable `urn:lenso:problem:<lowercase-hyphenated-code>`, for
example `urn:lenso:problem:conflict` (409, "Conflict", request refused). Each
occurrence has a locally generated opaque `urn:uuid:...` instance, never a
credential-bearing request URL. The body status agrees with the HTTP response.
`onProblem(problem)` can record this safe metadata and exact occurrence ID;
exceptions from this logging callback do not change the response.
This is an explicit endpoint representation, not Accept negotiation. Errors
after stream headers are committed still fail the stream; no late rewrite occurs.
For OpenAPI SSE, late iterator failures use oRPC's SSE error event representation,
not a new HTTP Problem Details response. The adapter sanitizes the producer's
error before the serializer sees it: only a known code and fixed public title
reach the event, never the original message/data/cause. Unknown failures become
opaque internal errors. The original error remains an internal cause; event
metadata, iterator cancellation/finalization and the committed HTTP status remain
unchanged. This applies to compact streams and detailed output's `body` stream.

For the official OpenAPI client, use the decoding helper, not `RPCLink`:

```ts
import { createProblemDetailsDecoder, createProblemDetailsFetch } from "@lenso/web/openapi-client";
import { OpenAPILink } from "@orpc/openapi/fetch";
import { createORPCClient } from "@orpc/client";

const link = new OpenAPILink(selectedRouter, {
  origin: "https://api.example.com",
  url: "/api",
  fetch: createProblemDetailsFetch(),
  customErrorResponseBodyDecoder: createProblemDetailsDecoder(),
});
const client = createORPCClient(link);
```

Supply the same `codes` definitions to the decoder for custom codes. It checks
media type, stable type URI, known code and actual status, and returns an
`ORPCError` with the configured fixed title, never a remote raw detail/data field.
Unexpected error representations become opaque 500 errors. Successful output
uses ordinary OpenAPI JSON semantics (use oRPC's `JsonifiedClient` type for dates
and other non-JSON native output). The `/rpc` client encoding remains unchanged.

The decoder alone cannot guard malformed error JSON: OpenAPILink parses the body
before calling `customErrorResponseBodyDecoder`. Opt in to
`createProblemDetailsFetch()` as `OpenAPILink.fetch` as shown above. It preserves
successful responses and streams untouched, bounds error-body reads to 8 KiB,
and returns only canonical safe Problem Details for recognized errors. Do not
wrap native `RPCLink` fetches with this OpenAPI-specific guard.

## Telemetry

Add `createORPCInstrumentation()` from `@lenso/otel/orpc` to the application's
single SDK bootstrap, not plugin setup. Its default `propagationEnabled: true`
uses oRPC v2's built-in propagation and handler/procedure/middleware/stream spans.
When HTTP/Fetch instrumentation already propagates context, explicitly set
`propagationEnabled: false`. This adapter adds instance/plugin attributes to
procedure spans, with no second HTTP server span or Web-specific SDK.

For raw handlers or detached work/cleanup, opt into
`telemetry: { requestLifetime: true }`. It owns incoming context extraction and
adds an **internal** `web.lifetime` span, not a duplicate server span, through
body settlement, registered work and finalizers. Pair it with oRPC
`propagationEnabled: false` and do not enable it when external HTTP/Fetch
instrumentation already owns the boundary. `response_ready`, `aborted` and
`cleanup_complete` distinguish headers, cancellation and actual resource
release. Body pulls and cleanup re-enter the captured async context.
Request metrics use only bounded HTTP method/status labels, never URLs or IDs.
The application chooses logging; no body, header, credential, URL query or
abort-reason text is logged. oRPC's own exception instrumentation can include
application error text: keep public/recorded errors safe and avoid secrets in
messages.

## Bun listener

`createBunListenerPlugin({ web, hostname, port, ingress })` owns a Bun listener
for the exact declared `web` plugin instance. `ingress` is required and is the
application's explicit ingress policy: return a `Response` to handle a request,
or `undefined` to pass it unchanged to `web.fetch`. `ingress(request, url)` receives
the actual listener URL, not the request's Host-derived URL, for Origin checks.
Its service exposes the listener's actual `url` and `port` (including when configured
with port `0`). Cleanup is registered immediately after setup acquires the server,
and app shutdown awaits `server.stop(true)`.
The Bun adapter is intentionally not exported from the Fetch-only package root.

```ts
const web = createWebPlugin({
  requires: [upstream],
  router: () => ({}),
  timeoutMs: 30_000,
  fetch: (plugins) => async (context) => {
    if (new URL(context.request.url).pathname !== "/stream") return undefined;
    const source = plugins.get(upstream);
    const resource = await source.open({ signal: context.signal });
    context.onCleanup(() => resource.close());
    context.waitUntil(resource.finished); // if a producer outlives its body
    return new Response(resource.body);
  },
});
```

`WebContext` contains `request`, `signal`, `onCleanup(callback)` and
`waitUntil(promise)`. Pass `signal` to upstream Fetch/providers. A returned
`Response` means headers are available, **not** that its body or producer ended.
Cleanup runs once, in reverse registration order, after EOF, error or completed
source cancellation and all registered work settles. Register resources as soon
as they are acquired. Register independently running producer work immediately.
The listener must propagate client disconnect through `Request.signal` or cancel
the response body; this has been tested with Bun 1.4.2.

The wrapper has zero read-ahead, one outstanding read, and rejects source chunks
above `maxChunkBytes` (default 65,536). This bounds its own buffering; configure
the upstream producer's queue/chunk size too. Runtime socket buffering and source
queues are separate. Avoid unbounded push producers or unconsumed clones/tees.
Consumers must drain or cancel the body.

`timeoutMs`, if specified, covers headers and body. Before headers, errors return
500, timeout returns 504, and cancellation returns 499. After headers, failures
error the body; an HTTP status cannot be replaced. `onError` receives only a
sanitized phase (`handler`, `body`, `work`, `cleanup`); cleanup failures do not
prevent other finalizers. Late responses are cancelled and late rejections are
observed. App stop rejects new requests with 503, signals active requests, and
waits for their actual finalization before dependent services are released.

Cancellation is cooperative. An upstream API that ignores signals/cancel may
keep running. `waitUntil` preserves ownership of that work; cleanup and app stop
then wait for real settlement, possibly indefinitely. A deadline does not prove
that the producer stopped, release a still-used resource, or kill that operation.

Official contracts: [oRPC Fetch](https://orpc.dev/docs/adapters/fetch-api),
[oRPC async iterators](https://orpc.dev/docs/async-iterator-object),
[Streams](https://streams.spec.whatwg.org/),
[Bun server](https://bun.com/docs/runtime/http/server).
