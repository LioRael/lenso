import { expect, test } from "bun:test";
import { startSchedulerDriver } from "../src/driver";

test("explicit driver does not overlap ticks and stop drains in-flight work", async () => {
  const entered = Promise.withResolvers<void>();
  const release = Promise.withResolvers<void>();
  let calls = 0;
  const driver = startSchedulerDriver(
    {
      async tick() {
        calls++;
        entered.resolve();
        await release.promise;
        return { advanced: 0, enqueued: 0, denied: 0, failed: 0 };
      },
    },
    { intervalMs: 1 },
  );
  await entered.promise;
  let drained = false;
  const stop = driver.stop().then(() => {
    drained = true;
  });
  await Bun.sleep(5);
  expect(drained).toBe(false);
  expect(calls).toBe(1);
  release.resolve();
  await stop;
  await driver.stop();
  expect(calls).toBe(1);
});

test("driver failures are observable and invalid configuration is rejected", async () => {
  const failure = new Error("fixture");
  const driver = startSchedulerDriver({
    tick: async () => {
      throw failure;
    },
  });
  await expect(driver.done).rejects.toBe(failure);
  await expect(driver.stop()).rejects.toBe(failure);
  expect(() =>
    startSchedulerDriver(
      { tick: async () => ({ advanced: 0, enqueued: 0, denied: 0, failed: 0 }) },
      { intervalMs: 0 },
    ),
  ).toThrow();
});
