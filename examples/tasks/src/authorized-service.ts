import {
  audience,
  createAuth,
  realm,
  type Auth,
  type AuthSource,
  type SubjectRef,
} from "@lenso/auth";
import type { TaskQueue } from "@lenso/tasks";
import { z } from "zod";
import { jobInput, reportQueryInput, submitInput } from "./contracts";
import type { OwnershipStore } from "./ownership";
import type { createReportService } from "./report-service";
import type { createReportTask } from "./task";

export interface TaskServiceResources {
  queue: Pick<TaskQueue, "enqueue" | "get" | "cancel" | "retry">;
  task: ReturnType<typeof createReportTask>;
  ownership: OwnershipStore;
  reports: Pick<ReturnType<typeof createReportService>, "get">;
}

// The standalone entry owns its Auth. Compositions with an existing Auth use
// createTaskService and retain cleanup in that Auth's plugin owner.
export function createAuthorizedTaskService(
  options: TaskServiceResources & {
    source: AuthSource<string | null>;
    evidence: () => string | null;
  },
) {
  const auth = createAuth(realm("task-example", options.source));
  return { ...createTaskService({ ...options, auth }), close: () => auth.close() };
}

export function createTaskService<R extends string, E, S extends string>(
  options: TaskServiceResources & { auth: Auth<R, E, S>; evidence: () => E },
) {
  const access = options.auth.for(audience("task-example.operations"));
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
