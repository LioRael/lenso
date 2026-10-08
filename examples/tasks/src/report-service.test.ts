import { expect, test } from "bun:test";
import { cooperativeDelay, reportInput, summarize } from "./report-service";

test("report input has defaults and a stable required business key", () => {
  expect(reportInput.parse({ reportId: "daily", rows: [2, 3] })).toEqual({
    reportId: "daily",
    rows: [2, 3],
    failUntilAttempt: 0,
    durationMs: 0,
  });
  expect(reportInput.safeParse({ reportId: "", rows: [1] }).success).toBe(false);
  expect(reportInput.safeParse({ reportId: "daily", rows: [Infinity] }).success).toBe(false);
});

test("report aggregates include zero and negative rows", () => {
  expect(summarize([2, -3, 0, 5])).toEqual({ sum: 4, count: 4 });
  expect(summarize([])).toEqual({ sum: 0, count: 0 });
});

test("cooperative delay rejects an already aborted signal", () => {
  const controller = new AbortController();
  controller.abort();
  expect(() => cooperativeDelay(10_000, controller.signal)).toThrow();
});

test("cooperative delay settles when cancelled, without waiting for its timer", async () => {
  const controller = new AbortController();
  const delay = cooperativeDelay(10_000, controller.signal);
  controller.abort();
  await expect(delay).rejects.toThrow("Report execution cancelled.");
});

test("cooperative delay resolves normally", async () => {
  await cooperativeDelay(0, new AbortController().signal);
});
