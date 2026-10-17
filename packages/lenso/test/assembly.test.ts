import { expect, test } from "bun:test";
import {
  collectPlugins,
  definePlugin,
  DiagnosticError,
  startApp,
  withApp,
  type Plugin,
} from "../src";

test("roots include shared transitive instances once and clean up in reverse order", async () => {
  const events: string[] = [];
  const dependency = definePlugin({
    id: "db",
    setup(context) {
      events.push("db");
      context.onCleanup(() => {
        events.push("close db");
      });
      return { value: 42 };
    },
  });
  const consumer = (id: string) =>
    definePlugin({
      id,
      requires: [dependency],
      setup(context) {
        events.push(id);
        context.onCleanup(() => {
          events.push(`close ${id}`);
        });
        return context.get(dependency).value;
      },
    });
  const a = consumer("a");
  const b = consumer("b");
  const app = { plugins: collectPlugins([a, b, dependency, a]) };
  expect(app.plugins).toEqual([dependency, a, b]);
  const result: number = await withApp(app, (running) => running.get(a));
  expect(result).toBe(42);
  expect(events).toEqual(["db", "a", "b", "close b", "close a", "close db"]);
  // Existing strict assembly still requires explicit registration.
  await expect(startApp({ plugins: [a] })).rejects.toBeInstanceOf(DiagnosticError);
});

test("collection rejects distinct same-ID dependencies and cycles before setup", () => {
  const a = definePlugin({
    id: "same",
    setup() {
      throw new Error("must not run");
    },
  });
  const b = definePlugin({
    id: "same",
    setup() {
      throw new Error("must not run");
    },
  });
  expect(() => collectPlugins([a, b])).toThrow(DiagnosticError);
  const edges: Plugin[] = [];
  const cycle = definePlugin({ id: "cycle", requires: edges, setup() {} });
  edges.push(cycle);
  expect(() => collectPlugins([cycle])).toThrow("Cyclic plugin dependency");
  expect(() => collectPlugins([null as unknown as Plugin])).toThrow(DiagnosticError);
});

test("finite lifetime preserves original work and cleanup errors, including thrown undefined", async () => {
  const cleanupError = new Error("cleanup");
  let closed = 0;
  const plugin = definePlugin({
    id: "owned",
    setup(context) {
      context.onCleanup(() => {
        closed++;
        throw cleanupError;
      });
    },
  });
  try {
    await withApp({ plugins: [plugin] }, () => {
      throw undefined;
    });
    throw new Error("expected failure");
  } catch (error) {
    expect(error).toBeInstanceOf(AggregateError);
    const failures = (error as AggregateError).errors;
    expect(failures[0]).toBeUndefined();
    expect(failures[1].errors).toEqual([cleanupError]);
  }
  expect(closed).toBe(1);
});

test("failed startup rolls back and never invokes work; failed work can be retried with a fresh app", async () => {
  let cleanups = 0;
  const failure = new Error("setup");
  const plugin = definePlugin({
    id: "resource",
    setup(context) {
      context.onCleanup(() => {
        cleanups++;
      });
      throw failure;
    },
  });
  let invoked = false;
  await expect(
    withApp({ plugins: [plugin] }, () => {
      invoked = true;
    }),
  ).rejects.toBe(failure);
  expect(invoked).toBe(false);
  expect(cleanups).toBe(1);
  const workError = new Error("work");
  await expect(
    withApp({ plugins: [] }, () => {
      throw workError;
    }),
  ).rejects.toBe(workError);
  expect(await withApp({ plugins: [] }, () => "retry")).toBe("retry");
});

test("successful work still reports cleanup failure and forwards startup options", async () => {
  const failure = new Error("close");
  const plugin = definePlugin({
    id: "resource",
    setup(context) {
      expect(context.instanceId).toBe("finite-app");
      context.onCleanup(() => {
        throw failure;
      });
      return 42;
    },
  });
  try {
    await withApp({ plugins: [plugin] }, (app) => app.get(plugin), { instanceId: "finite-app" });
    throw new Error("expected cleanup failure");
  } catch (error) {
    expect(error).toBeInstanceOf(AggregateError);
    expect((error as AggregateError).errors).toEqual([failure]);
  }
});
