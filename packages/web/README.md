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
