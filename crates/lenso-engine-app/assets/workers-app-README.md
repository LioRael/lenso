# Lenso local Workers App

This directory is a local-workerd build from one selected, verified Workers
Component Bundle. The `plan.mjs`, Component, Jco bindings, and pinned
`@lenso/workers-runtime` modules are self-contained. Their identities and
digests are recorded in `workers-build.json`; the selected Bundle and Host
authority are retained under `bundles/` and `.lenso/`.
For authoring V2, the embedded source descriptor must carry the exact trusted
HTTP Endpoint Descriptor digest, which is passed to the pinned runtime for a
Guest startup check. V1 retains the legacy no-digest descriptor shape.

Run locally with a compatible Wrangler installation:

```sh
wrangler dev --local --config wrangler.jsonc --ip 127.0.0.1 --port 8787
```

This target does not deploy, publish, or provision remote resources. It admits
only one request-only HTTP Endpoint with no dependencies, configuration, Host
imports, or supervision. HTTP bodies are bounded to 64 KiB; request and response
headers to 32 KiB and 128 coalesced fields. Session cookies,
WebSockets, streaming, scheduled events, queues, persistence, and network
egress are not wired. Do not use `wrangler deploy` as a qualification shortcut.
Guest deadlines, cancellation, and an aggregate linear-memory ceiling are not
enforced by this Host; it is not a production-equivalent Native Kernel path.
Fetch coalesces repeated raw request header fields, so raw-header parity is not
claimed. Percent-encoded URL paths are rejected rather than silently routed
under different decoding rules. Query strings are passed as exposed by Fetch.

The `workers-build.json` receipt is build evidence, not a signature or an
authorization to deploy. Rebuild after any source or Bundle change.
