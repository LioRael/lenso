# Borrowed MCP host

This offline example starts one Lenso application and borrows it across MCP
entries. `defineManage` declares existing async service operations;
`selectManageOperations` explicitly exposes only `read` and `write`. Zod strict
schemas reject extra business fields. The service enforces tenant and scope
authorization using bound trusted identity, not identity supplied in tool JSON.
Signals reach both bindings and the cooperative service.

From the repository root, using Bun 1.4.2 and the checked-in workspace lockfile:

```sh
bun install --frozen-lockfile
bun x turbo run build --filter=@lenso/mcp
bun run --cwd examples/mcp-host typecheck
bun run --cwd examples/mcp-host demo
bun test examples/mcp-host/test
```

`demo` uses the official SDK client and Streamable HTTP transport with an
in-memory fetch mount. It discovers tools, performs repeated reads and writes,
rejects cross-tenant access, closes the adapters, and calls `app.get(notes)`
successfully before the owner stops the app. No ports, network, paid services,
or external credentials are used. The short direct adapter example and HTTP
entry share the exact same running app and selected Operations.

The HTTP verifier accepts one **test-only fixture string** and returns a fixed
test-only identity. It is deliberately not production OAuth. A deployed host
must inject signature verification or introspection with issuer, resource
audience, expiry and current revocation checks. Do not replace this verifier
with JWT decoding or pass arbitrary client JSON as verified identity.

For optional integration, mount `http.fetch(request)` under the host's existing
router and retain ownership of the listener. HTTP does not redirect console or
stop the application. The demo's `127.0.0.1` resource and `.invalid` issuer are
test-only names, not production configuration.

For a dedicated local stdio process, configure an MCP host to launch:

```sh
bun /absolute/path/to/examples/mcp-host/src/stdio.ts
```

`stdio.ts` uses `serveBorrowedStdio` with the fixed test-only launch identity.
EOF and SIGINT/SIGTERM drain the adapter before the entry owner stops the app.
This is not remote authentication. Console output is redirected and redacted
by the adapter while active; direct stdout/native writes can still corrupt the
protocol in this trusted in-process model. Never write application logs to
stdout. Cancellation does not undo committed effects.
