import { expect, test } from "bun:test";
import { defineApp, definePlugin, startApp, type Logger } from "../src/index";

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
