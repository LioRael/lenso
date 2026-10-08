import { z } from "zod";
import { reportFailure } from "./config";
import { reportInput } from "./report-service";
import { openResources } from "./resources";

const enqueueInput = z
  .object({
    ...reportInput.shape,
    runAt: z.iso.datetime({ offset: true }).optional(),
    deduplicationKey: z.string().min(1).max(200).optional(),
  })
  .strict();

async function main() {
  const [command, id, ...extra] = process.argv.slice(2);
  if (
    extra.length ||
    !command ||
    (command === "enqueue" ? id !== undefined : !id) ||
    !["enqueue", "get", "cancel", "retry", "report"].includes(command)
  ) {
    console.error(
      "Usage: producer.ts enqueue < input.json | get|cancel|retry <jobId> | report <reportId>",
    );
    process.exitCode = 2;
    return;
  }
  const input =
    command === "enqueue" ? enqueueInput.parse(JSON.parse(await Bun.stdin.text())) : undefined;
  const resources = await openResources();
  try {
    switch (command) {
      case "enqueue": {
        const { runAt, deduplicationKey, ...payload } = input!;
        const jobId = await resources.queue.enqueue(resources.task, payload, {
          runAt: runAt ? new Date(runAt) : undefined,
          deduplicationKey,
        });
        console.log(jobId);
        break;
      }
      case "get": {
        const status = await resources.queue.get(id!);
        console.log(
          JSON.stringify(
            status
              ? {
                  state: status.state,
                  attempt: status.attempt,
                  maxAttempts: status.maxAttempts,
                  cancelRequested: status.cancelRequested,
                }
              : null,
          ),
        );
        break;
      }
      case "cancel":
        console.log(await resources.queue.cancel(id!));
        break;
      case "retry":
        console.log(await resources.queue.retry(id!));
        break;
      case "report":
        console.log(JSON.stringify(await resources.service.get(id!)));
        break;
    }
  } finally {
    await resources.close();
  }
}

if (import.meta.main) {
  await main().catch(() => reportFailure("Producer"));
}
