import type { Actor } from "@lenso/auth";
import type { Scheduler, ScheduleRule, ScheduleSummary } from "@lenso/scheduler";
import type { createPaymentsReconciliationTask } from "./tasks";
import { PaymentsError } from "./contracts";

/**
 * Explicit one-time provisioning, not startup work. Persist the returned schedule ID;
 * subsequent management uses Scheduler's authorized get/update/pause/cancel methods.
 */
export async function createPaymentsRecoverySchedule<A extends Actor>(
  options: {
    scheduler: Pick<Scheduler<A>, "create">;
    /** The exact reconciliation Task registered in both Scheduler and the existing Tasks queue. */
    task: ReturnType<typeof createPaymentsReconciliationTask>;
    rule: Extract<ScheduleRule, { kind: "cron" }>;
    limit?: number;
    graceMs?: number;
  },
  actor: A,
): Promise<ScheduleSummary> {
  const limit = options.limit ?? 50;
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 200 || options.rule.kind !== "cron")
    throw new PaymentsError("invalid-input");
  return options.scheduler.create(
    {
      task: options.task.name,
      input: { limit },
      rule: {
        kind: "cron",
        expression: options.rule.expression,
        timezone: options.rule.timezone,
      },
      misfire: "coalesce",
      graceMs: options.graceMs ?? 5_000,
    },
    actor,
  );
}
