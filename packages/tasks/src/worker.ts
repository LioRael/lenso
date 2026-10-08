import type { ClaimedJob, ExecutionResult, TaskWorker, WorkerOptions } from "./contracts";
import { TaskQueueError } from "./errors";

export interface WorkerClaim {
  readonly traceMetadata?: ClaimedJob["traceMetadata"];
  readonly jobId: string;
  readonly task: string;
  readonly input: ClaimedJob["input"];
  readonly attempt: number;
  readonly heartbeatMs: number;
  readonly retryCount: number;
}

export interface WorkerBackend {
  fetch(): Promise<WorkerClaim | null>;
  pulse(claim: WorkerClaim): Promise<{ owned: boolean; cancelRequested: boolean }>;
  settle(claim: WorkerClaim, result: ExecutionResult): Promise<void>;
}

function delay(ms: number, signal: AbortSignal): Promise<void> {
  if (signal.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    const finish = () => {
      clearTimeout(timer);
      signal.removeEventListener("abort", finish);
      resolve();
    };
    const timer = setTimeout(finish, ms);
    signal.addEventListener("abort", finish, { once: true });
  });
}

export function createTaskWorker(
  backend: WorkerBackend,
  execute: (job: ClaimedJob) => Promise<ExecutionResult>,
  options: WorkerOptions,
  pollIntervalMs: number,
): TaskWorker {
  const concurrency = options.concurrency ?? 1;
  if (!Number.isSafeInteger(concurrency) || concurrency < 1) {
    throw new Error("Task worker concurrency must be a positive integer");
  }
  if (
    options.timeoutMs !== undefined &&
    (!Number.isFinite(options.timeoutMs) || options.timeoutMs <= 0)
  ) {
    throw new Error("Task worker timeoutMs must be positive");
  }
  let stopping = false;
  let abortOnStop = false;
  const wake = new AbortController();
  const active = new Set<AbortController>();
  const failures: unknown[] = [];

  async function run(claim: WorkerClaim): Promise<void> {
    const controller = new AbortController();
    const heartbeatWake = new AbortController();
    active.add(controller);
    // A claim returned by an in-flight fetch after stop must not start new business work.
    if (stopping || abortOnStop) controller.abort();
    let lost = false;
    let settled = false;
    const pulse = async () => {
      try {
        const status = await backend.pulse(claim);
        if (!status.owned) lost = true;
        if (lost || status.cancelRequested) controller.abort();
      } catch {
        // A claim that cannot be verified must never be settled by this attempt.
        lost = true;
        controller.abort();
      }
    };
    let timeout: ReturnType<typeof setTimeout> | undefined;
    let heartbeat: Promise<void> | undefined;
    try {
      await pulse();
      if (options.timeoutMs !== undefined)
        timeout = setTimeout(() => controller.abort(), options.timeoutMs);
      heartbeat = (async () => {
        while (!settled && !lost) {
          await delay(claim.heartbeatMs, heartbeatWake.signal);
          if (!settled) await pulse();
        }
      })();
      let result: ExecutionResult;
      if (controller.signal.aborted) {
        result = { ok: false, error: "aborted" };
      } else {
        try {
          result = await execute({
            jobId: claim.jobId,
            task: claim.task,
            input: claim.input,
            traceMetadata: claim.traceMetadata,
            attempt: claim.attempt,
            signal: controller.signal,
          });
        } catch {
          result = { ok: false, error: "handler-failed" };
        }
      }
      if (controller.signal.aborted) result = { ok: false, error: "aborted" };
      settled = true;
      heartbeatWake.abort();
      await heartbeat;
      if (controller.signal.aborted) result = { ok: false, error: "aborted" };
      if (!lost) await backend.settle(claim, result);
    } finally {
      settled = true;
      heartbeatWake.abort();
      if (timeout !== undefined) clearTimeout(timeout);
      await heartbeat;
      active.delete(controller);
    }
  }

  async function lane(): Promise<void> {
    while (!stopping) {
      const claim = await backend.fetch();
      // A stop racing with fetch still owns the returned claim and must settle it.
      if (claim) await run(claim);
      else if (!stopping) await delay(pollIntervalMs, wake.signal);
    }
  }
  const lanes = Array.from({ length: concurrency }, () =>
    lane().catch((error: unknown) => {
      failures.push(error);
      stopping = true;
      wake.abort();
    }),
  );
  const done = Promise.all(lanes).then(() => {
    if (failures.length) throw new TaskQueueError("provider-unavailable");
  });
  // Hosts may begin observing after startWorker returns; still keep rejections handled.
  void done.catch(() => {});
  return {
    done,
    stop(stopOptions = {}) {
      stopping = true;
      wake.abort();
      if (stopOptions.abort) {
        abortOnStop = true;
        for (const controller of active) controller.abort();
      }
      return done;
    },
  };
}
