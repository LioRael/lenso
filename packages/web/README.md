# @lenso/web

Optional Fetch/oRPC adapter for Lenso. The application owns the HTTP listener,
or can opt into the Bun-specific listener adapter at `@lenso/web/bun`.
`createWebPlugin({ requires, router, prefix?, fetch?, timeoutMs?, maxChunkBytes?, onError? })`
exposes `WebService.fetch(request): Promise<Response>`. The default RPC prefix is `/rpc`.

`router(pluginContext)` builds the existing oRPC 1.15.5 router. Optional
`fetch(pluginContext)` builds a raw handler receiving `WebContext`; returning
`undefined` falls through to oRPC. Raw responses keep their status, headers and
bytes without token-by-token RPC encoding. Use oRPC's existing event iterator
when you actually need typed RPC events.

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

Official contracts: [oRPC v1 Fetch](https://v1.orpc.dev/docs/adapters/http),
[oRPC event iterators](https://v1.orpc.dev/docs/event-iterator),
[Streams](https://streams.spec.whatwg.org/),
[Bun server](https://bun.com/docs/runtime/http/server).
