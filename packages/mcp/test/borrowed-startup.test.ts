import { expect, spyOn, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { defineOperation } from "@lenso/engine/operations";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { z } from "zod";
import { acquireStdioOwnership, serveBorrowedStdio } from "../src/borrowed-stdio";
import { serveStdio } from "../src/index";

test("borrowed and legacy stdio share process ownership without changing legacy errors", async () => {
  const release = acquireStdioOwnership();
  const originalConsole = globalThis.console;
  try {
    await expect(
      serveStdio({ root: `${import.meta.dir}/fixtures`, allow: [] }),
    ).rejects.toMatchObject({
      diagnostic: { code: "invalid-arguments", phase: "arguments" },
    });
    expect(globalThis.console).toBe(originalConsole);
  } finally {
    release();
  }
});

test("borrowed stdio startup failure restores console/listeners/ownership, never stops app", async () => {
  let stopped = false;
  const plugin = definePlugin({
    id: "borrowed-startup",
    setup({ onCleanup }) {
      onCleanup(() => {
        stopped = true;
      });
      return { read: async (_input: Record<string, never>) => ({ usable: true }) };
    },
  });
  const operation = defineOperation({
    plugin,
    method: "read",
    input: z.strictObject({}),
    description: "Read.",
  });
  const app = await startApp({ plugins: [plugin] });
  const originalConsole = globalThis.console;
  const listeners = ["SIGINT", "SIGTERM"].map((name) => process.listenerCount(name));
  const startup = new Error("private startup error");
  const connect = spyOn(Server.prototype, "connect").mockImplementation(async () => {
    throw startup;
  });
  try {
    for (let i = 0; i < 2; i++) {
      await expect(
        serveBorrowedStdio({
          running: app,
          plugins: [plugin],
          operations: [operation],
          identity: { subject: "fixture" },
          canList: () => true,
          authorize: () => true,
          binding: () => ({}),
        }),
      ).rejects.toBe(startup);
      expect(globalThis.console).toBe(originalConsole);
      expect(["SIGINT", "SIGTERM"].map((name) => process.listenerCount(name))).toEqual(listeners);
      expect(stopped).toBe(false);
      expect(await app.get(plugin).read({})).toEqual({ usable: true });
    }
  } finally {
    connect.mockRestore();
    await app.stop();
  }
});
