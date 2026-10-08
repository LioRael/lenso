import { expect, spyOn, test } from "bun:test";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { serveStdio } from "../src";

test("startup and close double failure retain identities and restore process ownership", async () => {
  const startup = new Error("startup private");
  const cleanup = new Error("close private");
  const originalConsole = globalThis.console;
  const listeners = ["SIGINT", "SIGTERM"].map((name) => process.listenerCount(name));
  let closes = 0;
  const connect = spyOn(Server.prototype, "connect").mockImplementation(async () => {
    throw startup;
  });
  const close = spyOn(Server.prototype, "close").mockImplementation(async () => {
    closes++;
    throw cleanup;
  });
  try {
    for (let attempt = 0; attempt < 2; attempt++) {
      const failure = await serveStdio({ root: `${import.meta.dir}/fixtures`, allow: [] }).catch(
        (error) => error,
      );
      expect(failure).toBeInstanceOf(AggregateError);
      expect(failure.errors).toEqual([startup, cleanup]);
      expect(failure.errors[0]).toBe(startup);
      expect(failure.errors[1]).toBe(cleanup);
      expect(globalThis.console).toBe(originalConsole);
      expect(["SIGINT", "SIGTERM"].map((name) => process.listenerCount(name))).toEqual(listeners);
    }
    expect(closes).toBe(2);
  } finally {
    connect.mockRestore();
    close.mockRestore();
  }
});
