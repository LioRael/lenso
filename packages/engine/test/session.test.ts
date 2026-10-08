import { afterEach, expect, test } from "bun:test";
import { mkdtemp, realpath, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createEngineSession } from "../src/engine";
import { EngineSession } from "../src/engine-host";
import { EngineError } from "../src/diagnostics";
import type { Cleanup, EngineContext, EnginePlugin, Registration } from "../src/engine-authoring";

const directories: string[] = [];
afterEach(async () => {
  for (const directory of directories.splice(0))
    await rm(directory, { recursive: true, force: true });
});
async function fixture() {
  const root = await realpath(await mkdtemp(join(tmpdir(), "lenso-session-")));
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
  const cause = await promise.catch((failure: unknown) => failure);
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
  const cause = await close.catch((failure: unknown) => failure);
  expect(cause).toBeInstanceOf(EngineError);
  expect((cause as EngineError).diagnostic.code).toBe("engine-cleanup-failed");
  expect(((cause as EngineError).cause as AggregateError).errors).toEqual([cleanupFailure]);
  expect(session.close()).toBe(close);
  expect(await session.close().catch((failure: unknown) => failure)).toBe(cause);
});

test("registration revocation is immediate, identity-safe and never restores a replaced hook", async () => {
  const session = new EngineSession(await fixture(), "generate");
  let old!: Registration;
  let current!: Registration;
  const calls: string[] = [];
  await session.setup([
    ...defaults((c) => {
      old = c.generate("file", () => {
        calls.push("old");
        return [];
      });
    }),
    {
      name: "replacement",
      after: ["test"],
      setup(c) {
        current = c.generate(
          "file",
          () => {
            calls.push("current");
            return [];
          },
          { replace: "test" },
        );
      },
    },
  ]);
  await session.discover();
  old();
  old();
  await session.generate();
  expect(calls).toEqual(["current"]);
  current();
  current();
  await session.generate();
  expect(calls).toEqual(["current"]);
  await session.close();
  old();
  current();
});

test("each explicit watch owns one reference, independent of shared and static inputs", async () => {
  const root = await fixture();
  await Bun.write(join(root, "asset.txt"), "asset");
  const session = new EngineSession(root, "check");
  let first!: Registration;
  let repeated!: Registration;
  let shared!: Registration;
  let config!: Registration;
  await session.setup([
    ...defaults((c) => {
      first = c.watch("asset.txt");
      repeated = c.watch("asset.txt");
      config = c.watch("lenso.config.ts");
    }),
    {
      name: "shared",
      setup(c) {
        shared = c.watch("asset.txt");
      },
    },
  ]);
  await session.discover();
  first();
  first();
  repeated();
  expect(session.snapshot().watchFiles).toContain(join(root, "asset.txt"));
  shared();
  expect(session.snapshot().watchFiles).not.toContain(join(root, "asset.txt"));
  config();
  expect(session.snapshot().watchFiles).toContain(join(root, "lenso.config.ts"));
  await session.close();
  expect(session.snapshot().watchFiles).toEqual([]);
  expect(session.snapshot().sources).toEqual([]);
});

test("auto revocation follows registration LIFO and cleanup cannot register new work", async () => {
  const root = await fixture();
  await Bun.write(join(root, "asset.txt"), "asset");
  const session = new EngineSession(root, "check");
  const observations: boolean[] = [];
  let context!: EngineContext;
  await session.setup(
    defaults((c) => {
      context = c;
      c.onCleanup(() => {
        observations.push(session.snapshot().watchFiles.includes(join(root, "asset.txt")));
        expect(() => c.watch("asset.txt")).toThrow(EngineError);
        expect(() => c.onCleanup(() => {})).toThrow(EngineError);
        expect(() => c.generate("late", () => [])).toThrow(EngineError);
      });
      c.watch("asset.txt");
      c.onCleanup(() => {
        observations.push(session.snapshot().watchFiles.includes(join(root, "asset.txt")));
      });
    }),
  );
  await session.discover();
  await session.close();
  expect(observations).toEqual([true, false]);
  expect(() => context.watch("asset.txt")).toThrow(EngineError);
});

test("early async cleanup is joined by close and its rejection survives without a second execution", async () => {
  const session = new EngineSession(await fixture(), "check");
  const release = deferred();
  const failure = new Error("cleanup");
  let dispose!: Cleanup;
  let calls = 0;
  await session.setup(
    defaults((c) => {
      dispose = c.onCleanup(async () => {
        calls++;
        await release.promise;
        throw failure;
      });
    }),
  );
  const completion = dispose();
  const observed = completion.catch((error: unknown) => error);
  expect(dispose()).toBe(completion);
  let finished = false;
  const close = session.close();
  const closed = close.catch((error: unknown) => {
    finished = true;
    return error;
  });
  await Promise.resolve();
  expect(finished).toBe(false);
  release.resolve();
  expect(await observed).toBe(failure);
  const error = (await closed) as EngineError;
  expect((error.cause as AggregateError).errors).toEqual([failure]);
  expect(session.close()).toBe(close);
  expect(calls).toBe(1);
});

test("revoking a hook does not cancel its in-flight invocation but skips future hooks", async () => {
  const session = new EngineSession(await fixture(), "generate");
  const entered = deferred();
  const release = deferred();
  let first!: Registration;
  let second!: Registration;
  const calls: string[] = [];
  await session.setup(
    defaults((c) => {
      first = c.generate("first", async () => {
        calls.push("first");
        entered.resolve();
        await release.promise;
        return [];
      });
      second = c.generate("second", () => {
        calls.push("second");
        return [];
      });
    }),
  );
  await session.discover();
  const generation = session.generate();
  await entered.promise;
  first();
  second();
  release.resolve();
  await generation;
  await session.generate();
  expect(calls).toEqual(["first"]);
  await session.close();
});

test("a revoked selected target fails structurally instead of dereferencing a missing hook", async () => {
  const root = await fixture();
  const session = new EngineSession(root, "build");
  let revoke!: Registration;
  await session.setup([
    {
      name: "target",
      setup(c) {
        c.convention(() => ({ config: "lenso.config.ts" }));
        revoke = c.target("bun", () => "");
      },
    },
  ]);
  await session.discover();
  revoke();
  await expectCode(session.build(), "unknown-engine-target");
  await session.close();
});

test("a target revoked while validating its entry never starts", async () => {
  const session = new EngineSession(await fixture(), "build");
  let revoke!: Registration;
  let calls = 0;
  await session.setup([
    {
      name: "target",
      setup(c) {
        c.convention(() => ({ config: "lenso.config.ts" }));
        revoke = c.target("bun", (build) => {
          calls++;
          return build.bundle({ entry: build.entry });
        });
      },
    },
  ]);
  await session.discover();
  const building = session.build("lenso.config.ts");
  queueMicrotask(revoke);
  await expectCode(building, "unknown-engine-target");
  expect(calls).toBe(0);
  await session.close();
});
