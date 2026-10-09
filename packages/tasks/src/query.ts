import type { JobPage, JobQuery, JobSummary } from "./contracts";
import { TaskQueueError } from "./errors";

export function normalizeJobQuery(query: JobQuery): JobQuery & { limit: number } {
  if (
    !query ||
    !Array.isArray(query.tasks) ||
    query.tasks.length > 100 ||
    query.tasks.some(
      (task) => typeof task !== "string" || !/^[a-zA-Z][a-zA-Z0-9_.-]{0,127}$/.test(task),
    ) ||
    !Number.isInteger(query.limit ?? 50) ||
    (query.limit ?? 50) < 1 ||
    (query.limit ?? 50) > 100 ||
    (query.after !== undefined &&
      (typeof query.after !== "string" ||
        !/^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(query.after)))
  ) {
    throw new TaskQueueError("invalid-options");
  }
  return {
    tasks: [...new Set(query.tasks)],
    limit: query.limit ?? 50,
    ...(query.after === undefined ? {} : { after: query.after.toLowerCase() }),
  };
}

export function jobSummary(job: JobSummary): JobSummary {
  return {
    jobId: job.jobId,
    task: job.task,
    state: job.state,
    attempt: job.attempt,
    maxAttempts: job.maxAttempts,
    cancelRequested: job.cancelRequested,
  };
}

export function jobPage(rows: readonly JobSummary[], limit: number): JobPage {
  const items = rows.slice(0, limit).map(jobSummary);
  return {
    items,
    nextCursor: rows.length > limit ? items[items.length - 1]!.jobId : null,
  };
}
