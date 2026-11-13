import assert from "node:assert/strict";
import test from "node:test";
import { ConsoleClient } from "@lenso/console";

test("default and injected Console transports preserve the browser fetch receiver", async () => {
  const previous = globalThis.fetch;
  globalThis.fetch = async function () {
    assert.equal(this, globalThis, "browser fetch requires its global receiver");
    return Response.json({ version: 1, modules: [], pages: [] });
  };
  try {
    for (const client of [new ConsoleClient("http://localhost/console", () => "token"), new ConsoleClient("http://localhost/console", () => "token", globalThis.fetch)]) {
      assert.deepEqual(await client.manifest(), { version: 1, modules: [], pages: [] });
    }
  }
  finally { globalThis.fetch = previous; }
});
