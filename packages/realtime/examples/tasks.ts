import type { Realtime } from "@lenso/realtime";

/** Call only after the existing Tasks owner commits a status transition. */
export async function taskStatusCommitted(
  realtime: Realtime,
  resource: { scope: string; id: string },
  status: "queued" | "running" | "succeeded" | "failed",
) {
  return realtime.publish({ ...resource, type: "task" }, "task.status", { status });
}
