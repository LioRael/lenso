import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { definePlugin } from "@lenso/core";
import { EngineError } from "@lenso/engine/diagnostics";
import { defineOperation } from "../src/operations";
import { CliError, diagnostic, exitCode } from "../src/diagnostics";
import { invoke } from "../src/engine";

const input = {
  "~standard": {
    version: 1 as const,
    vendor: "cancellation-test",
    validate: (value: unknown) => ({ value }),
  },
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test("CLI abort across asynchronous binding waits for binding and cleans its own app", async () => {
  const entered = deferred<void>();
  const finish = deferred<void>();
  const controller = new AbortController();
  const reason = { message: "private-reason" };
  let calls = 0;
  let cleanup = 0;
  const plugin = definePlugin({
    id: "cli-binding",
    setup(lifecycle) {
      lifecycle.onCleanup(() => {
        cleanup++;
      });
      return {
        run(_input: unknown) {
          calls++;
          return true;
        },
      };
    },
  });
  const operation = defineOperation({ plugin, method: "run", input, description: "Run" });
  let settled = false;
  const call = invoke(
    { plugins: [plugin], operations: [operation] },
    plugin.id,
    "run",
    {},
    async () => {
      entered.resolve();
      await finish.promise;
      return { signal: controller.signal };
    },
  ).catch((cause: unknown) => cause);
  void call.then(() => {
    settled = true;
  });
  await entered.promise;
  controller.abort(reason);
  await Bun.sleep(0);
  expect(settled).toBe(false);
  expect(cleanup).toBe(0);
  finish.resolve();
  const error = await call;
  expect(error).toBeInstanceOf(CliError);
  expect(diagnostic(error)).toMatchObject({ code: "aborted", phase: "invoke" });
  expect(exitCode(error)).toBe(1);
  expect((error as CliError).cause).toBeInstanceOf(EngineError);
  expect(((error as CliError).cause as EngineError).cause).toBe(reason);
  expect(JSON.stringify(diagnostic(error))).not.toContain("private-reason");
  expect(calls).toBe(0);
  expect(cleanup).toBe(1);
});

test("CLI execution abort does not release resources or replace a business failure", async () => {
  for (const outcome of ["success", "reason", "business"] as const) {
    const entered = deferred<void>();
    const finish = deferred<void>();
    const controller = new AbortController();
    const reason = { message: "private-reason" };
    const business = new Error("private-business");
    let cleanup = 0;
    const plugin = definePlugin({
      id: "cli-executing",
      setup(lifecycle) {
        lifecycle.onCleanup(() => {
          cleanup++;
        });
        return {
          async run(_input: unknown) {
            entered.resolve();
            await finish.promise;
            if (outcome === "reason") controller.signal.throwIfAborted();
            if (outcome === "business") throw business;
            return true;
          },
        };
      },
    });
    const operation = defineOperation({ plugin, method: "run", input, description: "Run" });
    let settled = false;
    const call = invoke(
      { plugins: [plugin], operations: [operation] },
      plugin.id,
      "run",
      {},
      () => ({ signal: controller.signal }),
    ).catch((cause: unknown) => cause);
    void call.then(() => {
      settled = true;
    });
    await entered.promise;
    controller.abort(reason);
    await Bun.sleep(0);
    expect(settled).toBe(false);
    expect(cleanup).toBe(0);
    finish.resolve();
    const error = await call;
    expect(diagnostic(error).code).toBe(outcome === "business" ? "invocation-failed" : "aborted");
    expect(((error as CliError).cause as EngineError).cause).toBe(
      outcome === "business" ? business : reason,
    );
    expect(cleanup).toBe(1);
    expect(exitCode(error)).toBe(1);
    expect(JSON.stringify(diagnostic(error))).not.toContain("private-");
  }
});

test("CLI cancellation reasons cannot choose the public message or exit status", async () => {
  const reason = new CliError(
    { code: "private-reason", phase: "input", message: "private-reason" },
    2,
  );
  const controller = new AbortController();
  controller.abort(reason);
  let calls = 0;
  const plugin = definePlugin({
    id: "cli-custom-reason",
    setup: () => ({
      run(_input: unknown) {
        calls++;
        return true;
      },
    }),
  });
  const operation = defineOperation({ plugin, method: "run", input, description: "Run" });
  const error = await invoke(
    { plugins: [plugin], operations: [operation] },
    plugin.id,
    "run",
    {},
    () => ({ signal: controller.signal }),
  ).catch((cause: unknown) => cause);
  expect(exitCode(error)).toBe(1);
  expect(diagnostic(error)).toMatchObject({ code: "aborted", phase: "invoke" });
  expect(JSON.stringify(diagnostic(error))).not.toContain("private-reason");
  expect(calls).toBe(0);
});

test("JSON CLI stdin maps a real preaborted signal to aborted, exit 1 and zero business calls", async () => {
  const root = await mkdtemp(join(import.meta.dir, ".cancellation-"));
  try {
    await Bun.write(
      join(root, "lenso.config.ts"),
      `
      let calls = 0;
      const plugin = { id: "cancelled", setup({onCleanup}) {
        onCleanup(() => console.error("business-calls=" + calls));
        return { run() { calls++; return true; } };
      }};
      export const operations = [{
        plugin, method: "run", description: "Run",
        input: { "~standard": { version: 1, vendor: "test", validate: value => ({value}) } }
      }];
      export const operationBinding = () => {
        const controller = new AbortController();
        controller.abort({ message: "private-reason" });
        return { signal: controller.signal };
      };
      export default { plugins: [plugin] };
      `,
    );
    const child = Bun.spawn(
      [
        process.execPath,
        join(import.meta.dir, "../src/bin.ts"),
        "call",
        "cancelled",
        "run",
        "--root",
        root,
        "--stdin",
        "--json",
      ],
      { stdin: "pipe", stdout: "pipe", stderr: "pipe" },
    );
    child.stdin.write("{}");
    child.stdin.end();
    const [stdout, stderr, code] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    expect(code).toBe(1);
    expect(JSON.parse(stdout)).toMatchObject({
      schemaVersion: 1,
      ok: false,
      error: { code: "aborted", phase: "invoke" },
    });
    expect(stdout).not.toContain("private-reason");
    expect(stderr).toContain("business-calls=0");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
