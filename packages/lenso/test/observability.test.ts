import { expect, spyOn, test } from "bun:test";
import { metrics, trace } from "@opentelemetry/api";
import {
  bindConfig,
  defineApp,
  definePlugin,
  definePluginConfig,
  startApp,
  type Logger,
} from "../src/index";

test("failing lifecycle telemetry preserves results, LIFO failures and shared stop", async () => {
  const broken = () => {
    throw new Error("PRIVATE-telemetry");
  };
  const meter = spyOn(metrics, "getMeter").mockImplementation(broken);
  const tracer = spyOn(trace, "getTracer").mockReturnValue({
    startActiveSpan(...args: unknown[]) {
      return (args.at(-1) as (span: unknown) => unknown)({ setStatus: broken, end: broken });
    },
  } as unknown as ReturnType<typeof trace.getTracer>);
  const cleanup = new Error("original cleanup");
  const setup = new Error("original setup");
  const events: string[] = [];
  const success = definePlugin({
    id: "success",
    setup(context) {
      context.onCleanup(() => {
        events.push("first");
      });
      context.onCleanup(() => {
        events.push("second");
        throw cleanup;
      });
      return { ready: true };
    },
  });
  try {
    const app = await startApp({ plugins: [success] });
    expect(app.get(success)).toEqual({ ready: true });
    const stopped = app.stop();
    expect(app.stop()).toBe(stopped);
    const stopFailure = await stopped.catch((error) => error);
    expect(stopFailure).toBeInstanceOf(AggregateError);
    expect(stopFailure.errors[0]).toBe(cleanup);
    expect(events).toEqual(["second", "first"]);
    const failed = definePlugin({
      id: "failed",
      setup(context) {
        context.onCleanup(() => {
          throw cleanup;
        });
        throw setup;
      },
    });
    const failure = await startApp({ plugins: [failed] }).catch((error) => error);
    expect(failure).toBeInstanceOf(AggregateError);
    expect(failure.errors).toEqual([setup, cleanup]);
    tracer.mockImplementation(broken);
    let calls = 0;
    const untraced = definePlugin({
      id: "untraced",
      setup() {
        calls++;
        return true;
      },
    });
    const running = await startApp({ plugins: [untraced] });
    expect(running.get(untraced)).toBe(true);
    expect(calls).toBe(1);
    await running.stop();
  } finally {
    meter.mockRestore();
    tracer.mockRestore();
  }
});

test("instance defaults are unique and explicit entry options override app configuration", async () => {
  const instances: string[] = [];
  const plugin = definePlugin({
    id: "instance",
    setup(context) {
      instances.push(context.instanceId);
      return {};
    },
  });
  const first = await startApp(defineApp({ plugins: [plugin] }));
  const second = await startApp({ plugins: [plugin] });
  const explicit = await startApp(
    { plugins: [plugin], instanceId: "config" },
    { instanceId: "entry" },
  );
  expect(first.instanceId).not.toBe(second.instanceId);
  expect(first.instanceId).toMatch(/^[0-9a-f-]{36}$/);
  expect(explicit.instanceId).toBe("entry");
  expect(instances).toEqual([first.instanceId, second.instanceId, "entry"]);
  await Promise.all([first.stop(), second.stop(), explicit.stop()]);
});

test("a failing structural logger never replaces original setup or cleanup failures", async () => {
  const logger: Logger = {
    child() {
      throw new Error("logger unavailable");
    },
    debug() {
      throw new Error("logger unavailable");
    },
    info() {},
    warn() {},
    error() {
      throw new Error("logger unavailable");
    },
  };
  const failure = new Error("original failure");
  const plugin = definePlugin({
    id: "failure",
    setup(context) {
      expect(context.logger).toBeUndefined();
      context.onCleanup(() => {
        throw failure;
      });
      throw failure;
    },
  });
  await expect(startApp({ plugins: [plugin], logger })).rejects.toMatchObject({
    errors: [failure, failure],
  });
});

test("configuration preflight composes with instance/logger options without granting them to sources", async () => {
  let reads = 0;
  const bindings: Record<string, unknown>[] = [];
  const logger: Logger = {
    child(value) {
      expect(reads).toBe(1);
      bindings.push(value);
      return logger;
    },
    debug() {},
    info() {},
    warn() {},
    error() {},
  };
  const signal = new AbortController().signal;
  const plugin = bindConfig(
    definePluginConfig({
      schema: {
        "~standard": { version: 1, vendor: "test", validate: (value: unknown) => ({ value }) },
      },
    }),
    [
      {
        descriptor: { id: "memory", kind: "custom" },
        async read(context) {
          reads++;
          expect(Object.keys(context)).toEqual(["signal"]);
          expect(context.signal).toBe(signal);
          return { values: { enabled: true } };
        },
      },
    ],
    {
      id: "configured",
      setup(context, config) {
        expect(context.logger).toBe(logger);
        return { config, instanceId: context.instanceId };
      },
    },
  );
  const app = await startApp(defineApp({ plugins: [plugin], instanceId: "application" }), {
    instanceId: "entry",
    logger,
    signal,
  });
  try {
    expect(app.instanceId).toBe("entry");
    expect(app.logger).toBe(logger);
    expect(app.get(plugin)).toEqual({ config: { enabled: true }, instanceId: "entry" });
    expect(bindings).toEqual([{ instanceId: "entry" }, { pluginId: "configured" }]);
  } finally {
    await app.stop();
  }
});
