import assert from "node:assert/strict";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { createHttpMcp, createMcpAdapter } from "@lenso/mcp";
import { startHost, testOnlyIdentity, testOnlyResource } from "./host";

/** Offline fixture, not an OAuth credential or a production verifier. */
const testOnlyBearerFixture = "test-only-offline-fixture";

export async function runDemo() {
  const host = await startHost();
  let borrowed: Awaited<ReturnType<typeof createMcpAdapter>> | undefined;
  let http: Awaited<ReturnType<typeof createHttpMcp>> | undefined;
  const client = new Client({ name: "test-only-offline-client", version: "1.0.0" });
  const input = { tenant: testOnlyIdentity.tenant, id: "demo-note" };
  const context = {
    identity: testOnlyIdentity,
    requestId: crypto.randomUUID(),
    signal: new AbortController().signal,
  };
  try {
    borrowed = await createMcpAdapter(host.options);
    const catalog = await borrowed.listTools(context);
    const directRead = catalog.tools.find((tool) => tool.title === "host-notes.read");
    assert(directRead);
    assert.equal((await borrowed.callTool(directRead.name, input, context)).isError, undefined);
    await borrowed.close();

    http = await createHttpMcp({
      ...host.options,
      resource: testOnlyResource,
      authorizationServers: [testOnlyIdentity.issuer],
      requiredScopes: ["notes:read"],
      allowedOrigins: [],
      verifyToken(token, signal) {
        signal.throwIfAborted();
        if (token !== testOnlyBearerFixture) throw new Error("Test-only fixture rejected.");
        return testOnlyIdentity;
      },
    });
    const mounted = http;
    // Mount on the existing host's fetch router in a real application. Here the
    // official client calls it in memory: no listener, DNS, or live IDP traffic.
    const transport = new StreamableHTTPClientTransport(new URL(testOnlyResource), {
      requestInit: { headers: { authorization: `Bearer ${testOnlyBearerFixture}` } },
      fetch: (url, init) => mounted.fetch(new Request(url, init)),
    });
    await client.connect(transport);
    const { tools } = await client.listTools();
    assert.equal(tools.length, 2);
    const read = tools.find((tool) => tool.title === "host-notes.read");
    const write = tools.find((tool) => tool.title === "host-notes.write");
    assert(read && write);
    for (const text of ["first revision", "second revision"]) {
      assert.equal(
        (await client.callTool({ name: write.name, arguments: { ...input, text } })).isError,
        undefined,
      );
      const result = await client.callTool({ name: read.name, arguments: input });
      assert.equal(result.isError, undefined);
      assert.deepEqual(result.content, [
        { type: "text", text: JSON.stringify({ id: input.id, text }) },
      ]);
    }
    assert.equal(
      (
        await client.callTool({
          name: read.name,
          arguments: { ...input, tenant: "test-only-other-tenant" },
        })
      ).isError,
      true,
    );
    await transport.terminateSession();
    await client.close();
    await http.close();
    assert.deepEqual(await host.app.get(host.notes).read(input, context), {
      id: input.id,
      text: "second revision",
    });
    assert.deepEqual(host.lifecycle, { starts: 1, stops: 0 });
  } finally {
    // Each owned layer is cleaned up even if an earlier layer fails.
    try {
      await client.close();
    } finally {
      try {
        await http?.close();
      } finally {
        try {
          await borrowed?.close();
        } finally {
          await host.app.stop();
        }
      }
    }
  }
  assert.deepEqual(host.lifecycle, { starts: 1, stops: 1 });
  return { tools: 2, revisions: 2, starts: host.lifecycle.starts, stops: host.lifecycle.stops };
}

if (import.meta.main) console.log(await runDemo());
