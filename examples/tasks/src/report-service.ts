import { eq } from "drizzle-orm";
import type { NodePgDatabase } from "drizzle-orm/node-postgres";
import { z } from "zod";
import * as schema from "./schema";

export const reportInput = z
  .object({
    reportId: z.string().min(1).max(200),
    rows: z.array(z.number().finite().min(-1e12).max(1e12)).max(10_000),
    failUntilAttempt: z.number().int().min(0).max(1_000).default(0),
    durationMs: z.number().int().min(0).max(3_600_000).default(0),
  })
  .strict();

export type ReportInput = z.output<typeof reportInput>;

export function summarize(rows: readonly number[]) {
  return { sum: rows.reduce((sum, row) => sum + row, 0), count: rows.length };
}

export function cooperativeDelay(durationMs: number, signal: AbortSignal): Promise<void> {
  signal.throwIfAborted();
  return new Promise((resolve, reject) => {
    const abort = () => {
      clearTimeout(timer);
      signal.removeEventListener("abort", abort);
      reject(new Error("Report execution cancelled."));
    };
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", abort);
      resolve();
    }, durationMs);
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort();
  });
}

// The caller owns db and its pool; this ordinary service never closes either.
export function createReportService(db: NodePgDatabase<typeof schema>) {
  return {
    async generate(input: ReportInput, context: { attempt: number; signal: AbortSignal }) {
      await cooperativeDelay(input.durationMs, context.signal);
      if (context.attempt <= input.failUntilAttempt) {
        throw new Error("Intentional report attempt failure.");
      }
      context.signal.throwIfAborted();
      const result = { reportId: input.reportId, ...summarize(input.rows) };
      await db
        .insert(schema.reports)
        .values({ ...result, updatedAt: new Date() })
        .onConflictDoUpdate({
          target: schema.reports.reportId,
          set: { sum: result.sum, count: result.count, updatedAt: new Date() },
        });
      return result;
    },
    async get(reportId: string) {
      const [row] = await db
        .select({ sum: schema.reports.sum, count: schema.reports.count })
        .from(schema.reports)
        .where(eq(schema.reports.reportId, reportId));
      return row ?? null;
    },
  };
}
