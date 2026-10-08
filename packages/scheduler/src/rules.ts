import { CronExpressionParser } from "cron-parser";
import type { Schedule, ScheduleRule } from "./contracts";
import { SchedulerError } from "./errors";

export function timestamp(value: number): number {
  if (!Number.isSafeInteger(value) || value < 0 || value > 8_000_000_000_000_000)
    throw new SchedulerError("invalid-input");
  return value;
}

function cron(rule: Extract<ScheduleRule, { kind: "cron" }>, after: number) {
  if (
    typeof rule.expression !== "string" ||
    !rule.expression.trim() ||
    rule.expression.length > 256 ||
    /(^|[\s,])H/i.test(rule.expression) ||
    typeof rule.timezone !== "string" ||
    !rule.timezone
  )
    throw new SchedulerError("invalid-input");
  const timezone = new Intl.DateTimeFormat("en", { timeZone: rule.timezone }).resolvedOptions()
    .timeZone;
  return CronExpressionParser.parse(rule.expression, {
    currentDate: new Date(after),
    tz: timezone,
  });
}

/** Cron is strictly after the instant; once is an absolute Unix millisecond instant. */
export function nextTrigger(rule: ScheduleRule, after: number): number | null {
  timestamp(after);
  try {
    if (rule.kind === "once") return timestamp(rule.at) > after ? rule.at : null;
    if (rule.kind !== "cron") throw new SchedulerError("invalid-input");
    return cron(rule, after).next().getTime();
  } catch {
    throw new SchedulerError("invalid-input");
  }
}

/** No unbounded walk over missed cron instants: at most one catch-up per schedule. */
export function planDue(schedule: Schedule, now: number) {
  timestamp(now);
  if (schedule.state !== "active" || schedule.nextAt === null || schedule.nextAt > now) return null;
  const misfired = now - schedule.nextAt > schedule.graceMs;
  return {
    scheduledAt: misfired && schedule.misfire === "skip" ? null : schedule.nextAt,
    nextAt: nextTrigger(schedule.rule, now),
  };
}
