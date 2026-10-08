import { expect, test } from "bun:test";
import { nextTrigger } from "../src/index";
import { planDue } from "../src/rules";
import type { Schedule } from "../src/contracts";

test("absolute one-time rules and explicit timezone validation", () => {
  expect(nextTrigger({ kind: "once", at: 2000 }, 1000)).toBe(2000);
  expect(nextTrigger({ kind: "once", at: 1000 }, 1000)).toBeNull();
  for (const rule of [
    { kind: "cron", expression: "", timezone: "UTC" },
    { kind: "cron", expression: "H * * * *", timezone: "UTC" },
    { kind: "cron", expression: "* * * * *", timezone: "Unknown/Zone" },
    { kind: "cron", expression: "60 * * * *", timezone: "UTC" },
    { kind: "once", at: NaN },
  ] as const)
    expect(() => nextTrigger(rule, 1000)).toThrow();
  expect(
    nextTrigger(
      { kind: "cron", expression: "0 0 L * *", timezone: "UTC" },
      Date.parse("2024-02-01T00:00:00Z"),
    ),
  ).toBe(Date.parse("2024-02-29T00:00:00Z"));
});

test("pinned cron-parser shifts a missing 02:30 to 03:30, then returns to local 02:30", () => {
  const rule = { kind: "cron", expression: "30 2 * * *", timezone: "America/New_York" } as const;
  const first = nextTrigger(rule, Date.parse("2024-03-09T08:00:00Z"))!;
  expect(first).toBe(Date.parse("2024-03-10T07:30:00Z"));
  expect(nextTrigger(rule, first)).toBe(Date.parse("2024-03-11T06:30:00Z"));
});

test("pinned cron-parser fold behavior is explicit for forward traversal and later-fold cursors", () => {
  const rule = { kind: "cron", expression: "30 1 * * *", timezone: "America/New_York" } as const;
  const first = nextTrigger(rule, Date.parse("2024-11-02T08:00:00Z"))!;
  expect(first).toBe(Date.parse("2024-11-03T05:30:00Z"));
  expect(nextTrigger(rule, first)).toBe(Date.parse("2024-11-04T06:30:00Z"));
  expect(nextTrigger(rule, Date.parse("2024-11-03T06:15:00Z"))).toBe(
    Date.parse("2024-11-03T06:30:00Z"),
  );
});

test("skip and coalesce bound years of misses to zero or one occurrence", () => {
  const due = Date.parse("2020-01-01T00:00:00Z");
  const now = Date.parse("2025-01-01T00:00:00Z");
  const schedule: Schedule = {
    id: crypto.randomUUID(),
    revision: 1,
    state: "active",
    nextAt: due,
    task: "fixture",
    input: null,
    initiator: { realmId: "fixture", subjectId: "alice" },
    rule: { kind: "cron", expression: "* * * * * *", timezone: "UTC" },
    misfire: "skip",
    graceMs: 100,
  };
  expect(planDue(schedule, now)).toEqual({ scheduledAt: null, nextAt: now + 1000 });
  expect(planDue({ ...schedule, misfire: "coalesce" }, now)).toEqual({
    scheduledAt: due,
    nextAt: now + 1000,
  });
  expect(planDue(schedule, due + 100)).toEqual({ scheduledAt: due, nextAt: due + 1000 });
  expect(planDue({ ...schedule, state: "paused" }, now)).toBeNull();
});
