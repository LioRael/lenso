import { expect, test } from "bun:test";
import { startApp } from "@lenso/core";
import { batch, batchPlugins } from "./batch-app";
import type { AggregateInput } from "../.lenso/runtime/logs";

const input: AggregateInput = {
  logs: [
    { level: "error", message: "failed", duration_ms: 12 },
    { level: "info", message: "done", duration_ms: 8 },
  ],
};

test("batch aggregation caches equivalent input and preserves its result", async () => {
  const app = await startApp({ plugins: batchPlugins });
  try {
    const service = app.get(batch);
    const [first, second] = await Promise.all([service.aggregate(input), service.aggregate(input)]);
    expect(first).toEqual({ count: 2, error_count: 1, total_duration_ms: 20 });
    expect(second).toEqual(first);
  } finally {
    await app.stop();
  }
});

test("batch aggregation rejects empty or cancelled calls and retained service use after stop", async () => {
  const app = await startApp({ plugins: batchPlugins });
  const service = app.get(batch);
  await expect(service.aggregate({ logs: [] })).rejects.toThrow("At least one log is required");
  await expect(service.aggregate(input, AbortSignal.abort())).rejects.toThrow();
  await app.stop();
  await expect(service.aggregate(input)).rejects.toThrow("Batch service is closed");
});

test("batch loader uses the same immutable input snapshot as its cache key", async () => {
  const app = await startApp({ plugins: batchPlugins });
  try {
    const service = app.get(batch);
    const mutable: AggregateInput = { logs: input.logs.map((record) => ({ ...record })) };
    const pending = service.aggregate(mutable);
    mutable.logs[0]!.duration_ms = 999;
    expect(await pending).toEqual({ count: 2, error_count: 1, total_duration_ms: 20 });
    expect(await service.aggregate(input)).toEqual({
      count: 2,
      error_count: 1,
      total_duration_ms: 20,
    });
  } finally {
    await app.stop();
  }
});
