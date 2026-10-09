import { defineTask, type TaskContext } from "@lenso/tasks";
import type { StandardSchemaV1 } from "@standard-schema/spec";
import type { MediaTasks } from "./contracts";
import { MediaError } from "./errors";

const input: StandardSchemaV1<{ derivationId: string }> = {
  "~standard": {
    version: 1,
    vendor: "@lenso/media",
    validate(value) {
      if (
        !value ||
        typeof value !== "object" ||
        Array.isArray(value) ||
        Object.keys(value).length !== 1 ||
        !("derivationId" in value) ||
        typeof value.derivationId !== "string" ||
        !/^[a-f0-9]{64}$/.test(value.derivationId)
      ) {
        return { issues: [{ message: "Expected an internal derivation identity." }] };
      }
      return { value: { derivationId: value.derivationId } };
    },
  },
};

export function createMediaTask(options: {
  name?: string;
  execute(id: string, context: TaskContext): Promise<void>;
}) {
  return defineTask({
    name: options.name ?? "media.process",
    input,
    maxAttempts: 3,
    retry: { delaySeconds: 2, backoff: true, maxDelaySeconds: 60 },
    handler: (value, context) => options.execute(value.derivationId, context),
    // Business status is in Media; no files, credentials, or URLs in Tasks results.
  });
}
export function createTasksMediaAdapter(
  queue: Pick<
    ReturnType<typeof import("@lenso/tasks").createTaskQueue>,
    "identity" | "enqueue" | "lookupDeduplicationKey" | "get" | "cancel" | "retry"
  >,
  task: ReturnType<typeof createMediaTask>,
): MediaTasks {
  const key = (id: string) => `${task.name}:${id}`;
  async function lookup(id: string) {
    if (!/^[a-f0-9]{64}$/.test(id)) throw new MediaError("invalid-input");
    return queue.lookupDeduplicationKey(key(id));
  }
  return {
    async identity() {
      const identity = await queue.identity();
      return `${identity.kind}:${identity.id}:${task.name}`;
    },
    async ensure(id) {
      const previous = await lookup(id);
      if (previous) return previous.jobId;
      return queue.enqueue(task, { derivationId: id }, { deduplicationKey: key(id) });
    },
    async job(id) {
      return (await lookup(id))?.status ?? null;
    },
    async cancel(id) {
      const previous = await lookup(id);
      return previous ? queue.cancel(previous.jobId) : "missing";
    },
    async retry(id) {
      const previous = await lookup(id);
      return previous ? queue.retry(previous.jobId) : false;
    },
  };
}
