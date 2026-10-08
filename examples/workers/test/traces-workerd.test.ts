import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { URL } from "node:url";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { createORPCClient } from "@orpc/client";
import { RPCLink } from "@orpc/client/fetch";
import type { Client } from "@orpc/client";

test("Workers Traces entry serves v2 RPC without a Node SDK", { timeout: 30_000 }, async () => {
  const script = await readFile(new URL("../.lenso/workerd/index.js", import.meta.url), "utf8");
  assert.equal(script.includes("@opentelemetry/sdk"), false);
  assert.equal(script.includes("node:async_hooks"), false);
  const runtime = new Miniflare({
    ...convertV4MiniflareOptions({
      modules: true,
      script,
      compatibilityDate: "2026-10-08",
      compatibilityFlags: ["nodejs_compat", "enable_request_signal"],
      bindings: { GREETING_PREFIX: "Hello" },
    }),
    host: "127.0.0.1",
    port: 0,
    telemetry: { enabled: false },
  });
  try {
    const url = new URL((await runtime.ready).href);
    const client: {
      greet: Client<
        Record<never, never>,
        { name: string },
        { message: string; count: number },
        never
      >;
    } = createORPCClient(new RPCLink({ origin: url.origin, url: "/rpc" }));
    assert.deepEqual(await client.greet({ name: "Ada" }), { message: "Hello, Ada!", count: 1 });
    await assert.rejects(client.greet({ name: "x" }), { code: "BAD_REQUEST" });
  } finally {
    await runtime.dispose();
  }
});
