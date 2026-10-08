import { expect, test } from "bun:test";
import { defineSource } from "@lenso/auth";
import { createAuthorizedTaskService } from "./authorized-service";
import { openResources } from "./resources";

// Deliberately no migration here. Use the explicit example migration on a test DB first.
test.skipIf(!process.env.TASK_TEST_DATABASE_URL)(
  "PostgreSQL durable owners, cancel and failed retry",
  async () => {
    const config = {
      connectionString: process.env.TASK_TEST_DATABASE_URL!,
      queueName: "authorization-test",
    };
    const first = await openResources(config);
    try {
      const second = await openResources(config);
      try {
        const source = defineSource<string | null>({
          async verify(evidence) {
            return evidence === "test-owner" || evidence === "test-other"
              ? { status: "verified", subjectId: evidence }
              : { status: "rejected" };
          },
        });
        const make = (resources: typeof first, evidence: string) =>
          createAuthorizedTaskService({
            ...resources,
            reports: resources.service,
            source,
            evidence: () => evidence,
          });
        const owner = make(first, "test-owner");
        const restarted = make(second, "test-owner");
        const other = make(second, "test-other");
        try {
          const id = `authorization-${crypto.randomUUID()}`;
          const cancelled = await owner.submit({
            reportId: `${id}-cancel`,
            rows: [],
            runAt: new Date(Date.now() + 60_000).toISOString(),
          });
          expect(await restarted.query(cancelled)).toMatchObject({ state: "pending" });
          for (const method of ["query", "cancel", "retry"] as const) {
            await expect(other[method](cancelled)).rejects.toMatchObject({ code: "FORBIDDEN" });
          }
          await expect(
            other.submit({ reportId: `${id}-cancel`, rows: [99] }),
          ).rejects.toMatchObject({ code: "FORBIDDEN" });
          expect(await restarted.cancel(cancelled)).toBe("cancelled");
          expect(await restarted.cancel(cancelled)).toBe("terminal");
          const retry = await owner.submit({
            reportId: `${id}-retry`,
            rows: [8, 9],
            failUntilAttempt: 3,
          });
          const worker = await first.queue.startWorker();
          try {
            async function waitFor(job: { jobId: string }, state: string) {
              const deadline = Date.now() + 30_000;
              while (Date.now() < deadline) {
                const status = await restarted.query(job);
                if (status?.state === state) return status;
                await Bun.sleep(50);
              }
              throw new Error("Test job did not reach expected state.");
            }
            const running = await owner.submit({
              reportId: `${id}-running`,
              rows: [1],
              durationMs: 10_000,
            });
            await waitFor(running, "running");
            await expect(other.cancel(running)).rejects.toMatchObject({ code: "FORBIDDEN" });
            const cancellation = await restarted.cancel(running);
            expect(cancellation).toBe("requested");
            const afterRequest = await restarted.query(running);
            expect(["running", "cancelled"]).toContain(afterRequest?.state ?? "missing");
            if (afterRequest?.state === "running") expect(afterRequest.cancelRequested).toBe(true);
            const settled = await waitFor(running, "cancelled");
            console.log({ cancellation, afterRequest, settled });
            expect(await restarted.report({ reportId: `${id}-running` })).toBeNull();
            expect(await waitFor(retry, "failed")).toMatchObject({ attempt: 3 });
            expect(await restarted.retry(retry)).toBe(true);
            expect(await waitFor(retry, "succeeded")).toMatchObject({ attempt: 4 });
            expect(await restarted.report({ reportId: `${id}-retry` })).toEqual({
              sum: 17,
              count: 2,
            });
            expect(await restarted.retry(retry)).toBe(false);
          } finally {
            await worker.stop({ abort: true });
          }
        } finally {
          await Promise.all([owner.close(), restarted.close(), other.close()]);
        }
      } finally {
        await second.close();
      }
    } finally {
      await first.close();
    }
  },
  40_000,
);
