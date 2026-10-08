import { defineTask } from "@lenso/tasks";
import { reportInput, type createReportService } from "./report-service";

export function createReportTask(service: ReturnType<typeof createReportService>) {
  return defineTask({
    name: "generate-report",
    input: reportInput,
    maxAttempts: 3,
    retry: { delaySeconds: 2, backoff: true, maxDelaySeconds: 10 },
    async handler(input, { jobId, attempt, signal }) {
      console.error(JSON.stringify({ event: "report-started", jobId, attempt }));
      return service.generate(input, { attempt, signal });
    },
    result: (returned) => ({ sum: returned.sum, count: returned.count }),
  });
}
