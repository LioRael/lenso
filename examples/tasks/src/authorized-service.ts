import { audience, createAuth, realm, type AuthSource, type SubjectRef } from "@lenso/auth";
import type { TaskQueue } from "@lenso/tasks";
import { z } from "zod";
import { jobInput, reportQueryInput, submitInput } from "./contracts";
import type { OwnershipStore } from "./ownership";
import type { createReportService } from "./report-service";
import type { createReportTask } from "./task";

export function createAuthorizedTaskService(options: {
  source: AuthSource<string | null>;
  evidence: () => string | null;
  queue: Pick<TaskQueue, "enqueue" | "get" | "cancel" | "retry">;
  task: ReturnType<typeof createReportTask>;
  ownership: OwnershipStore;
  reports: Pick<ReturnType<typeof createReportService>, "get">;
}) {
  const auth = createAuth(realm("task-example", options.source));
  const access = auth.for(audience("task-example.operations"));
  const ownerPolicy = ({
    principal,
    resource,
  }: {
    principal: SubjectRef;
    resource: SubjectRef | null;
  }) =>
    resource !== null &&
    principal.realmId === resource.realmId &&
    principal.subjectId === resource.subjectId;
  const actor = () => access.required(options.evidence());
  async function authorizeJob(jobId: string) {
    const principal = await actor();
    await access.enforce(principal, await options.ownership.job(jobId), ownerPolicy);
  }
  return {
    close: () => auth.close(),
    async submit(input: z.input<typeof submitInput>) {
      const { runAt, deduplicationKey, ...payload } = submitInput.parse(input);
      const principal = await actor();
      // enforce before reserving a business key; a revoked session cannot reserve ownership.
      await access.enforce(principal, principal, ownerPolicy);
      const owner = await options.ownership.claim(payload.reportId, {
        realmId: principal.realmId,
        subjectId: principal.subjectId,
      });
      await access.enforce(principal, owner, ownerPolicy);
      const scopedKey =
        deduplicationKey === undefined
          ? undefined
          : new Bun.CryptoHasher("sha256")
              .update(
                JSON.stringify([
                  principal.realmId,
                  principal.subjectId,
                  payload.reportId,
                  deduplicationKey,
                ]),
              )
              .digest("hex");
      const jobId = await options.queue.enqueue(options.task, payload, {
        runAt: runAt ? new Date(runAt) : undefined,
        deduplicationKey: scopedKey,
      });
      // Queue and business DB are separate transactions. Failure here fails closed:
      // an orphan job is never accessible solely by knowing its ID.
      await options.ownership.record(jobId, payload.reportId);
      return { jobId };
    },
    async query(input: z.input<typeof jobInput>) {
      const { jobId } = jobInput.parse(input);
      await authorizeJob(jobId);
      const status = await options.queue.get(jobId);
      return status
        ? {
            state: status.state,
            attempt: status.attempt,
            maxAttempts: status.maxAttempts,
            cancelRequested: status.cancelRequested,
          }
        : null;
    },
    async cancel(input: z.input<typeof jobInput>) {
      const { jobId } = jobInput.parse(input);
      await authorizeJob(jobId);
      return options.queue.cancel(jobId);
    },
    async retry(input: z.input<typeof jobInput>) {
      const { jobId } = jobInput.parse(input);
      await authorizeJob(jobId);
      return options.queue.retry(jobId);
    },
    async report(input: z.input<typeof reportQueryInput>) {
      const { reportId } = reportQueryInput.parse(input);
      const principal = await actor();
      await access.enforce(principal, await options.ownership.report(reportId), ownerPolicy);
      return options.reports.get(reportId);
    },
  };
}
