import { afterEach, expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createEngineSession } from "../src/engine";
import { EngineSession } from "../src/engine-host";
import { EngineError } from "../src/diagnostics";
import type { EngineContext, EnginePlugin } from "../src/engine-authoring";

const directories: string[] = [];
afterEach(async () => {
  for (const directory of directories.splice(0))
    await rm(directory, { recursive: true, force: true });
});
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "lenso-session-"));
  directories.push(root);
  await Bun.write(join(root, "lenso.config.ts"), "export default { plugins: [] };");
  return root;
}
function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function defaults(setup?: (context: EngineContext) => void | Promise<void>): EnginePlugin[] {
  return [
    {
      name: "test",
      async setup(context) {
        context.convention(() => ({ config: "lenso.config.ts" }));
        context.target("bun", () => join(context.root, "dist"));
        await setup?.(context);
      },
    },
  ];
}
async function expectCode(promise: Promise<unknown>, code: string) {
  const cause = await promise.catch((cause: unknown) => cause);
  expect(cause).toBeInstanceOf(EngineError);
  expect((cause as EngineError).diagnostic.code).toBe(code);
}

test("prepare caches one promise for concurrent and repeated calls", async () => {
  const engine = createEngineSession(await fixture(), "check");
  const first = engine.prepare();
  expect(engine.prepare()).toBe(first);
  const app = await first;
  expect(engine.prepare()).toBe(first);
  expect(await engine.prepare()).toBe(app);
  await engine.session.close();
});

test("setup runs once and rejects incompatible defaults", async () => {
  const session = new EngineSession(await fixture(), "check");
  let calls = 0;
  const plugins = defaults(() => {
    calls++;
  });
  const first = session.setup(plugins);
  expect(session.setup([...plugins])).toBe(first);
  await first;
  expect(session.setup(plugins)).toBe(first);
  await expectCode(session.setup(defaults()), "invalid-engine-state");
  expect(calls).toBe(1);
  await session.close();
});

test("close waits awaited setup and drains later registrations in LIFO order", async () => {
  const session = new EngineSession(await fixture(), "check");
  const entered = deferred();
  const release = deferred();
  const cleaned: string[] = [];
  let context!: EngineContext;
  const setup = session.setup(
    defaults(async (c) => {
      context = c;
      c.onCleanup(() => {
        cleaned.push("first");
      });
      entered.resolve();
      await release.promise;
      c.watch("lenso.config.ts");
      c.onCleanup(() => {
        cleaned.push("later");
      });
    }),
  );
  await entered.promise;
  const close = session.close();
  expect(session.close()).toBe(close);
  await expectCode(session.setup([]), "engine-session-closed");
  await expectCode(session.discover(), "engine-session-closed");
  expect(cleaned).toEqual([]);
  release.resolve();
  await setup;
  await close;
  expect(cleaned).toEqual(["later", "first"]);
  expect(() => context.onCleanup(() => {})).toThrow(EngineError);
  expect(() => context.watch("lenso.config.ts")).toThrow(EngineError);
});

test("closed sessions reject stages before hooks or generated writes", async () => {
  const root = await fixture();
  const session = new EngineSession(root, "generate");
  let calls = 0;
  await session.setup(
    defaults((c) => {
      c.generate("file", () => {
        calls++;
        return [{ path: "file.ts", content: "output" }];
      });
      c.dev("start", () => {
        calls++;
      });
    }),
  );
  await session.discover();
  await session.close();
  await expectCode(session.discover(), "engine-session-closed");
  await expectCode(session.generate(), "engine-session-closed");
  await expectCode(session.build(), "engine-session-closed");
  await expectCode(session.dev("ready"), "engine-session-closed");
  expect(calls).toBe(0);
  expect(await Bun.file(join(root, ".lenso/.engine-files.json")).exists()).toBe(false);
  expect(await Bun.file(join(root, ".lenso/file.ts")).exists()).toBe(false);
});

test("stage prerequisites fail structurally even without registered hooks", async () => {
  const session = new EngineSession(await fixture(), "check");
  expect(() => session.snapshot()).toThrow(EngineError);
  await expectCode(session.discover(), "invalid-engine-state");
  await expectCode(session.generate(), "invalid-engine-state");
  await expectCode(session.build(), "invalid-engine-state");
  await expectCode(session.dev("ready"), "invalid-engine-state");
  await session.setup(defaults());
  await expectCode(session.generate(), "invalid-engine-state");
  await expectCode(session.build(), "invalid-engine-state");
  await expectCode(session.dev("ready"), "invalid-engine-state");
  await session.close();
  const missing = new EngineSession(await fixture(), "check");
  await missing.setup([
    {
      name: "target-only",
      setup(c) {
        c.target("bun", () => "");
      },
    },
  ]);
  await expectCode(missing.discover(), "invalid-engine-state");
  await missing.close();
});

test("close waits running stages, captured callbacks stay usable until cleanup", async () => {
  const root = await fixture();
  const session = new EngineSession(root, "generate");
  const entered = deferred();
  const release = deferred();
  const events: string[] = [];
  await session.setup(
    defaults((c) => {
      c.onCleanup(() => {
        events.push("setup-cleanup");
      });
      c.generate("file", async () => {
        entered.resolve();
        await release.promise;
        c.watch("lenso.config.ts");
        c.onCleanup(() => {
          events.push("hook-cleanup");
        });
        events.push("generated");
        return [{ path: "file.ts", content: "output" }];
      });
    }),
  );
  await session.discover();
  const generation = session.generate();
  await entered.promise;
  await expectCode(session.dev("ready"), "invalid-engine-state");
  const close = session.close();
  await expectCode(session.generate(), "engine-session-closed");
  expect(events).toEqual([]);
  release.resolve();
  await generation;
  await close;
  expect(events).toEqual(["generated", "hook-cleanup", "setup-cleanup"]);
  expect(await Bun.file(join(root, ".lenso/file.ts")).text()).toBe("output");
});

test("failed setup stays cached and close preserves cleanup failures", async () => {
  const session = new EngineSession(await fixture(), "check");
  const cleanupFailure = new Error("cleanup");
  let calls = 0;
  const plugins = defaults((c) => {
    calls++;
    c.onCleanup(() => {
      throw cleanupFailure;
    });
    throw new Error("setup");
  });
  const setup = session.setup(plugins);
  await expectCode(setup, "engine-hook-failed");
  expect(session.setup(plugins)).toBe(setup);
  expect(calls).toBe(1);
  const close = session.close();
  const cause = await close.catch((cause: unknown) => cause);
  expect(cause).toBeInstanceOf(EngineError);
  expect((cause as EngineError).diagnostic.code).toBe("engine-cleanup-failed");
  expect(((cause as EngineError).cause as AggregateError).errors).toEqual([cleanupFailure]);
  expect(session.close()).toBe(close);
  expect(await session.close().catch((cause: unknown) => cause)).toBe(cause);
});
