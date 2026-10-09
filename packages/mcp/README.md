# MCP adapters

`@lenso/mcp` projects **explicitly selected existing Operations**, never inferred
service methods. It offers a borrowed runtime adapter, dedicated local stdio,
and an opt-in authenticated Fetch Streamable HTTP entry. HTTP starts no listener
and does not change host logging. No business handlers, OAuth authorization
server, shell, Agent loop, tasks, or durable cancellation are supplied.

## Borrow one running app

```ts
import { createMcpAdapter, serveBorrowedStdio } from "@lenso/mcp";
import { selectManageOperations } from "@lenso/manage";

const shared = {
  running: app, // already started by its owner
  plugins: [notes], // exact installed instances
  operations: selectManageOperations(notesManage, ["read", "write"]),
  canList: (operation, request) => policy.canSee(request.identity, operation),
  authorize: (operation, request) => policy.canInvoke(request.identity, operation),
  binding: (_operation, _input, request) => ({
    context: { identity: request.identity, requestId: request.requestId, signal: request.signal },
  }),
};
const adapter = await createMcpAdapter(shared);
// adapter.listTools({identity, requestId, signal})
// adapter.callTool(discoveredName, input, {identity, requestId, signal})
await adapter.close(); // app remains usable; its owner alone stops it
```

`running` is the existing Engine `OperationRuntime`; `binding` returns existing
`OperationBoundOptions`, including confirmation/approval callbacks where required.
It must match the real service's second parameter (`context: true`). The request
context is trusted entry data, not a new registration or actor-in-JSON protocol.
Every call rechecks both visibility and invocation permission. The ordinary
service still enforces its tenant/object/owner rules. Manage/`createAgentTools`
are the canonical schema/metadata source; all selected schemas must convert to
SDK-compatible object schemas before readiness, including currently hidden tools.

For a **dedicated local process**, replace the adapter creation above with:

```ts
const stdio = await serveBorrowedStdio({ ...shared, identity: trustedLaunchIdentity });
// On shutdown await stdio.close(), then let the entry owner stop app.
```

It reuses that app for all calls, reserves stdio process ownership, routes console
logs to redacted stderr, and drains on EOF/SIGINT/SIGTERM. Start the app with a
stderr logger: the adapter cannot redirect logs emitted before it starts.
Never embed this stdio owner in a general-purpose HTTP host.

## Explicit HTTP mount and identity

```ts
import { createHttpMcp } from "@lenso/mcp";
const mcp = await createHttpMcp({
  ...shared,
  resource: "https://api.example.com/mcp",
  authorizationServers: ["https://idp.example.com"],
  requiredScopes: ["mcp:use"],
  allowedOrigins: ["https://trusted-client.example.com"], // [] denies present Origins
  verifyToken: hostIdentityProvider.verifyMcpAccessToken,
});
// Mount both /mcp and /.well-known/oauth-protected-resource/mcp:
// host Fetch router delegates these paths to mcp.fetch(request).
// Host owns its existing listener, TLS, CORS and reverse-proxy configuration.
```

`verifyToken(token, signal)` is mandatory. Inject the host's real OAuth/OIDC
signature verifier or token introspection client; decoding a JWT is not
verification. For JWT providers, a host-installed `jose` verifier can use
`createRemoteJWKSet` and `jwtVerify` with a fixed issuer, this resource audience,
and an explicit algorithm allowlist. Require `exp`; honor `nbf`, provider
revocation/current grants, and abortable verification. Map the **verified**
subject to the host's tenant and Lenso business identity. Do not trust a
client-selected tenant claim without the provider/host membership policy.
When services require an `@lenso/auth` Actor, retain the original verified Actor
reference in an identity extension such as `identity.actor` and bind that exact
reference. A copied MCP identity is not an Auth Actor or permission grant.

Return `McpIdentity`: `{subject, tenant, issuer, audience: string[], scopes:
string[], expiresAt}` (Unix seconds), optionally extending it with verified
business evidence. The adapter rechecks issuer, resource audience, expiry and
required scopes on **every** protected request, including notifications and
DELETE. `canList`, `authorize` and service policy handle current business grants;
discovery is not call permission. Configure the issuer string exactly, including
any provider-required trailing slash. Production verification/revocation remains
the host's responsibility, not the example's deterministic fixture verifier.

Missing/invalid credentials return 401 and a Bearer `resource_metadata` challenge;
missing required scopes return 403 with `insufficient_scope`. Public RFC 9728
metadata advertises the configured resource and authorization servers. Tokens
are neither forwarded to services nor to a different resource. Session IDs never
authenticate. Sessions and rate buckets bind issuer/subject/tenant/resource;
foreign owners receive 404 and grants are not cached in sessions.

Only canonical configured Host/Origin values are accepted. HTTPS is required
except on loopback; forwarded headers are not trusted. A proxy host must present
the canonical public Request URL through its own trusted routing. The adapter
starts no second listener; an independent listener must explicitly bind loopback
unless the host deliberately configures remote exposure.

## Borrowed-entry bounds and protocol support

Defaults: four admitted calls/discoveries (no queue), 30s request timeout,
256 KiB business input/catalog, 1 MiB business result, 1 MiB HTTP frame, and
2 MiB + 4 KiB complete HTTP response (including cumulative internal stream data).
HTTP additionally admits 32 requests, 128 sessions, 5-minute idle expiry, and
120 requests/minute per identity/tenant with at most 1024 rate buckets.
Trusted options can change these budgets; output/HTTP error budgets must fit a
256-byte fixed diagnostic. HTTP's outer verification/body/transport deadline is
the request timeout plus 1s. Expired sessions/buckets are swept on ingress.

Cancellation and timeout reach binding signals; only a host-bound cooperative
service can interrupt actual work. Noncooperative work retains its concurrency
slot until settlement. `close()` stops admission, aborts owned request signals,
drains actual work and releases protocol resources; it is idempotent and never
calls `app.stop()`. A hanging noncooperative service/verifier can prolong drain.
Neither cancellation nor a lost response promises rollback or safe retry.
HTTP disconnect alone is deliberately not an MCP cancellation notification.

Protocol failures use fixed SDK MCP errors. Validation, authorization, business
failure, cancellation, timeout, busy/closed and size errors have fixed bounded
tool diagnostics; arbitrary thrown text, stacks, input and credentials are omitted.
Engine's finite JSON/redaction policy remains in force, not a sandbox.

Official SDK **1.32.1** remains exactly pinned. Official-client tests verify
`2025-11-25` and `2025-03-26` Streamable HTTP, plus existing stdio. SDK owns
negotiation, messages, session IDs and notification validation. Public HTTP
responses are finite JSON; GET returns 405 (no standalone SSE), DELETE terminates
sessions. No HTTP+SSE legacy endpoint, replay/event store, progress, resources,
prompts, elicitation or task capability is claimed.

Two version-specific mitigations use SDK public APIs: its finite POST SSE
transport is consumed within a budget and normalized to JSON because 1.32.1
JSON mode retains completed stream resolvers; entry-scoped cancellation handlers
handle valid IDs `0`/`""` and emit finite replies so cancellation does not retain
HTTP waiters. Regression tests inspect retained SDK state, cancellation and
escape-heavy JSON. No SDK private state is patched.

See the [offline host example](../../examples/mcp-host/README.md) for copyable
workspace commands and discovery/multiple read/write calls with one app.
These new exports are source-workspace changes, not proof of registry publication.

## Existing trusted local launch (unchanged)

Install the optional package in the application and create a dedicated entry:

```ts
import { serveStdio } from "@lenso/mcp";

const adapter = await serveStdio({
  root: import.meta.dir,
  allow: [{ pluginId: "greeting", method: "greet" }],
});

// For explicit host shutdown, await adapter.close().
```

Configure an MCP host to launch `bun /absolute/path/to/mcp.ts`. The application
root and allowlist belong to trusted host configuration, not model/tool input.
There is deliberately no general-purpose command-line operation selector or
dynamic config-module loader. Use a fresh process when changing source or the
allowlist: normal trusted module caching still applies.

Startup calls `inspect(root)` from `@lenso/cli` without application setup. Every
allowlisted `(pluginId, method)` must already be declared. Duplicate, absent,
runtime-validation-only, and non-object input schemas reject startup before
protocol readiness. The adapter preserves Engine's converted input schema; it
does not invent an empty schema or infer one from runtime service methods.
Schemas must also satisfy the SDK's tool representation.

Tool names are `operation_0`, `operation_1`, etc., in allowlist order. Their
titles identify the canonical plugin/method. Clients should discover names
through `tools/list`, not persist an index across launch configuration changes.
Only those names resolve to fixed bindings; business arguments cannot select
the root, module, method, or shell. Applications still own the meaning and
safety of their business input.

## Invocation, authorization, and output

Each call delegates to `call(root, pluginId, method, input)` from `@lenso/cli`. The
existing shared Standard Schema validator runs before setup; the existing
service method runs with its service as `this`; CLI awaits app shutdown,
including ordered business/cleanup diagnostics. No alternate execution graph,
retry loop, HTTP actor, or authorization bypass exists here. Service
authorization must remain in the application. A host allowlist is additional
exposure policy, **not permission to bypass service authorization**.

All requests in one stdio process use the trusted launch environment's identity.
There is no per-request remote login or serialized-actor adapter. Run separate
processes with separately authorized credentials when local callers require
identity isolation. A developer who can change application code/config or use
the raw DB has administrative authority outside the tool surface; this adapter
does not grant that authority to its client. Remote credentials, audiences and
scopes belong to the explicitly enabled HTTP entry, never this stdio entry.

Successful tool content contains one text block with Engine's deterministic,
redacted JSON result. No output schema is fabricated. Undefined, cyclic,
nonfinite, and nonplain output fails through the existing serialization policy.
Business failures use `isError: true` with safe Engine/CLI diagnostic JSON,
without raw input, arbitrary exception text, or stacks. Unknown tool names
produce a protocol `InvalidParams` error; malformed messages are SDK-owned.
Explicit application `CliError` messages must themselves be safe.

Descriptions come from canonical operation metadata. MCP hints default
conservatively: read-only only for an explicit read effect, destructive unless
explicitly false, idempotent only for declared safe retry, open-world true.
Hints are descriptive, not authorization or execution guarantees. Output
descriptions are included when declared. Cancellation is always request-only
at this adapter boundary, even if application metadata says otherwise.
The full canonical description, including instance, source, schema availability
and semantic metadata, is retained in `_meta["lenso/operation"]`.

## Limits and shutdown

Defaults are a 1 MiB SDK incoming read buffer, 256 KiB serialized business
input, and 1 MiB redacted serialized result. Trusted options `maxFrameBytes`,
`maxInputBytes`, and `maxOutputBytes` must be positive safe integers. These
bound incoming framing and business payloads, not arbitrary allocations inside
trusted application code or tool-discovery metadata.
The output budget also bounds failed tool content. Oversized diagnostics retain
their code and phase with `truncated: true`, omitting paths, details, causes and
source to fit the limit. If even that header cannot fit, a fixed
`output-too-large` diagnostic is returned. `maxOutputBytes` must be at least 91
bytes so that fallback always fits. Serialization failures likewise use a safe
bounded diagnostic, never arbitrary error text.

Only one business call is admitted at a time; concurrent calls fail with
`adapter-busy`, without a queue. The adapter checks SDK request cancellation
before and after the awaited CLI call. The existing service API has no signal:
cancelling a request does **not** stop in-flight service work, undo effects, or
promise rollback. The client/SDK may discard the response. The adapter still
awaits the call and cleanup before admitting another operation.

`close()`, stdin EOF, SIGINT, and SIGTERM stop admission and drain the admitted
call before closing transport and restoring console. No forced-termination or
cleanup timeout promise is made. Hanging application work can hang shutdown;
a host that force-kills the process cannot rely on cleanup completion.
Detached tasks and acquired resources remain application-owned.

## stdout and trust

SDK protocol messages are the adapter's only stdout output. During startup,
operation execution, and drain, console methods (including Bun's
`console.write`) route to stderr. Common logging methods use the existing
redaction policy and omit direct Error text. This is a **trusted in-process
adapter, not a sandbox**: direct `process.stdout.write`, native logging,
captured pre-start console references, or deliberately unsafe code can still
corrupt protocol output or disclose secrets. Redaction cannot identify every
secret under innocuous keys. Do not embed secrets in schema constraints or
metadata, and do not print them.

## SDK and specification verification

Primary sources checked for this implementation:

- [npm registry: official SDK latest 1.x](https://registry.npmjs.org/@modelcontextprotocol/sdk/latest),
  pinned here to `1.32.1` (not a floating range).
- [Official SDK source, v1.x](https://github.com/modelcontextprotocol/typescript-sdk/tree/v1.x):
  its protocol latest constant is `2025-11-25`; the SDK owns negotiation.
- [Current MCP specification](https://modelcontextprotocol.io/specification/latest)
  resolves to `2026-07-28`. The current SDK v2 uses split packages.
- [2025-11-25 stdio transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
  and [tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).

This package deliberately uses the mature official monolithic v1 SDK. It does
**not** claim implementation of the newer 2026-07-28 stateless protocol or
v2-only features. No resources, prompts, tasks or elicitation are advertised.
HTTP uses the [2025-11-25 authorization specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)
and RFC 9728 protected-resource metadata, not an application login endpoint.

## Development

The integration owner installs dependencies and owns the workspace lockfile.
Rebuild `@lenso/core`, `@lenso/engine`, `@lenso/web`, `@lenso/auth`,
`@lenso/manage`, and `@lenso/cli` before consuming their exports.
Then run from this package:

```sh
bun run typecheck
bun run build
bun test
```

Tests launch a real subprocess and communicate using the official SDK client
and stdio transport. They cover discovery without setup, allowlisting, shared
validation, service lifecycle, redaction, safe authorization/runtime
diagnostics, unsupported output, payload limits, cancellation, and rejected
concurrent requests.
