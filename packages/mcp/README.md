# Local MCP adapter

`@lenso/mcp` is an optional local stdio adapter for **existing, explicitly
declared Lenso operations**. It uses the official MCP SDK's `Server` and
`StdioServerTransport`. It does not start a remote server, accept credentials,
implement business handlers, load Engine config, or expose a shell.

## Trusted launch configuration

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

Startup calls `lenso-cli.inspect(root)` without application setup. Every
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

Each call delegates to `lenso-cli.call(root, pluginId, method, input)`. The
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
scopes require a separately designed trusted ingress, not this stdio entry.

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
v2-only features. No resources, prompts, tasks, elicitation, remote auth, or
credentials are advertised.

## Development

The integration owner installs dependencies and owns the workspace lockfile.
Rebuild `lenso`, `@lenso/engine`, and `lenso-cli` before consuming their exports.
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
