import { describe, expect, test } from "bun:test";
import { defineApp, definePlugin, DiagnosticError, startApp, validatePlugins } from "../src";
import type { Plugin, PluginContext } from "../src";

describe("static plugin diagnostics", () => {
  test("dependency order and independently identified instances", async () => {
    const first = definePlugin({ id: "counter:first", setup: () => ({ count: 1 }) });
    const second = definePlugin({ id: "counter:second", setup: () => ({ count: 2 }) });
    const consumer = definePlugin({
      id: "consumer",
      requires: [first, second],
      setup: (context) => context.get(first).count + context.get(second).count,
    });
    expect(validatePlugins([consumer, second, first]).map((plugin) => plugin.id)).toEqual([
      "counter:first",
      "counter:second",
      "consumer",
    ]);
    const app = await startApp(defineApp({ plugins: [consumer, second, first] }));
    expect(app.get(consumer)).toBe(3);
    expect(app.get(first)).not.toBe(app.get(second));
    await app.stop();
  });

  test("reports duplicate IDs and exact-instance missing dependencies before setup", async () => {
    let started = false;
    const original = definePlugin({ id: "resource", setup: () => null });
    const impostor = definePlugin({ id: "resource", setup: () => null });
    const consumer = definePlugin({
      id: "consumer",
      requires: [original],
      setup: () => {
        started = true;
      },
    });
    try {
      await startApp({ plugins: [impostor, impostor, consumer] });
      throw new Error("Expected static diagnostics.");
    } catch (error) {
      expect(error).toBeInstanceOf(DiagnosticError);
      expect((error as DiagnosticError).diagnostics.map((diagnostic) => diagnostic.code)).toEqual([
        "duplicate-id",
        "missing-dependency",
      ]);
    }
    expect(started).toBe(false);
  });

  test("reports a readable dependency cycle and rejects an empty ID", () => {
    const a: Plugin = { id: "a", requires: [], setup: () => null };
    const b: Plugin = { id: "b", requires: [a], setup: () => null };
    (a.requires as Plugin[]).push(b);
    expect(() => validatePlugins([a, b])).toThrow("a -> b -> a");
    expect(() => validatePlugins([{ id: " ", setup: () => null }])).toThrow(
      "Plugin IDs must not be empty",
    );
  });
});

describe("plugin lifetime", () => {
  test("async setup and cleanup run in dependency/LIFO order; concurrent stop is idempotent", async () => {
    const events: string[] = [];
    const database = definePlugin({
      id: "database",
      setup: async ({ onCleanup }) => {
        events.push("database setup");
        onCleanup(async () => {
          await Promise.resolve();
          events.push("database close");
        });
        return { value: 42 };
      },
    });
    const feature = definePlugin({
      id: "feature",
      requires: [database],
      setup: async (context) => {
        events.push("feature setup");
        context.onCleanup(() => {
          events.push("feature first close");
        });
        context.onCleanup(() => {
          events.push("feature second close");
        });
        return context.get(database).value;
      },
    });
    const app = await startApp({ plugins: [feature, database] });
    expect(app.get(feature)).toBe(42);
    expect(app.status()).toEqual([
      { id: "database", state: "ready" },
      { id: "feature", state: "ready" },
    ]);
    const stopping = app.stop();
    expect(app.stop()).toBe(stopping);
    await stopping;
    await app.stop();
    expect(events).toEqual([
      "database setup",
      "feature setup",
      "feature second close",
      "feature first close",
      "database close",
    ]);
    expect(app.status().every((status) => status.state === "stopped")).toBe(true);
    expect(() => app.get(feature)).toThrow("The app is stopped");
  });

  test("initialization failure releases resources acquired by the failing plugin", async () => {
    const events: string[] = [];
    const failure = new Error("setup failed");
    const good = definePlugin({
      id: "good",
      setup: ({ onCleanup }) => {
        onCleanup(() => {
          events.push("good");
        });
      },
    });
    const bad = definePlugin({
      id: "bad",
      requires: [good],
      setup: async ({ onCleanup }) => {
        onCleanup(async () => {
          events.push("bad");
        });
        throw failure;
      },
    });
    const never = definePlugin({
      id: "never",
      requires: [bad],
      setup: () => {
        events.push("never setup");
      },
    });
    await expect(startApp({ plugins: [never, bad, good] })).rejects.toBe(failure);
    expect(events).toEqual(["bad", "good"]);
  });

  test("all finalizers run despite failure and preserve setup plus rollback errors", async () => {
    const events: string[] = [];
    const setupFailure = new Error("setup failure");
    const cleanupFailure = new Error("cleanup failure");
    const bad = definePlugin({
      id: "bad",
      setup: ({ onCleanup }) => {
        onCleanup(() => {
          events.push("remaining cleanup");
        });
        onCleanup(() => {
          events.push("failed cleanup");
          throw cleanupFailure;
        });
        throw setupFailure;
      },
    });
    try {
      await startApp({ plugins: [bad] });
      throw new Error("Expected failure.");
    } catch (error) {
      expect(error).toBeInstanceOf(AggregateError);
      expect((error as AggregateError).errors).toEqual([setupFailure, cleanupFailure]);
    }
    expect(events).toEqual(["failed cleanup", "remaining cleanup"]);
  });

  test("shutdown attempts every async/sync finalizer and returns the same failure on repeated stop", async () => {
    const events: string[] = [];
    const firstFailure = new Error("first");
    const secondFailure = new Error("second");
    const plugin = definePlugin({
      id: "cleanup failures",
      setup: ({ onCleanup }) => {
        onCleanup(() => {
          events.push("success");
        });
        onCleanup(() => {
          throw firstFailure;
        });
        onCleanup(async () => {
          throw secondFailure;
        });
      },
    });
    const app = await startApp({ plugins: [plugin] });
    const stopping = app.stop();
    try {
      await stopping;
    } catch (error) {
      expect((error as AggregateError).errors).toEqual([secondFailure, firstFailure]);
    }
    expect(app.stop()).toBe(stopping);
    await expect(app.stop()).rejects.toBeInstanceOf(AggregateError);
    expect(events).toEqual(["success"]);
  });

  test("rejects undeclared dependencies and rolls back allocated resources", async () => {
    let closed = false;
    const service = definePlugin({ id: "service", setup: () => 42 });
    const rogue = definePlugin({
      id: "rogue",
      setup: (context) => {
        context.onCleanup(() => {
          closed = true;
        });
        return context.get(service);
      },
    });
    await expect(startApp({ plugins: [service, rogue] })).rejects.toThrow(
      'undeclared dependency "service"',
    );
    expect(closed).toBe(true);
  });

  test("contributions retain plugin order and runtime lookup uses instance identity", async () => {
    const contribution = { kind: "example.metadata", id: "example" };
    const plugin = definePlugin({
      id: "example",
      contributions: [contribution, { kind: "other" }],
      setup: () => undefined,
    });
    const app = await startApp({ plugins: [plugin] });
    expect(app.get(plugin)).toBeUndefined();
    expect(app.contributions("example.metadata")).toEqual([contribution]);
    expect(app.contributions()).toHaveLength(2);
    expect(() => app.get({ ...plugin })).toThrow("not part of this app");
    await app.stop();
  });

  test("late cleanup registration cannot leak an acquired resource into a closed app", async () => {
    let context: PluginContext | undefined;
    const plugin = definePlugin({
      id: "late",
      setup: (setupContext) => {
        context = setupContext;
      },
    });
    const app = await startApp({ plugins: [plugin] });
    await app.stop();
    expect(() => context!.onCleanup(() => {})).toThrow("no longer active");
  });

  test("ordinary business methods can resolve declared dependencies until shutdown", async () => {
    const dependency = definePlugin({ id: "dependency", setup: () => 42 });
    const business = definePlugin({
      id: "business",
      requires: [dependency],
      setup: (context) => ({
        run: async () => context.get(dependency),
      }),
    });
    const app = await startApp({ plugins: [business, dependency] });
    const service = app.get(business);
    expect(await service.run()).toBe(42);
    await app.stop();
    await expect(service.run()).rejects.toThrow("The app is stopped");
  });
});

test("synchronous cleanup reentry observes the cached stop Promise", async () => {
  let reentered: Promise<void> | undefined;
  const plugin = definePlugin({
    id: "reentrant",
    setup({ onCleanup }) {
      onCleanup(() => {
        reentered = running.stop();
      });
    },
  });
  const running = await startApp({ plugins: [plugin] });
  const stopping = running.stop();
  await stopping;
  expect(reentered).toBe(stopping);
});
