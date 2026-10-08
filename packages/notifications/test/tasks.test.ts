import { afterEach, expect, it } from "bun:test";
import {
  createTaskQueue,
  type ClaimedJob,
  type ExecutionResult,
  type JobStatus,
  type ProviderJob,
  type TaskProvider,
} from "@lenso/tasks";
import { createNotificationDispatcher, createNotificationTask } from "../src/tasks";
import { createNotificationService } from "../src/service";
import { input, localFixture, template } from "./helpers";

/** Deterministic queue fixture; this does not verify the PostgreSQL Tasks backend. */
class QueueFixture implements TaskProvider {
  failEnqueue = false;
  job: ProviderJob | undefined;
  status: JobStatus | null = null;
  execute: ((job: ClaimedJob) => Promise<ExecutionResult>) | undefined;
  retried = 0;
  async enqueue(job: ProviderJob) {
    if (this.failEnqueue) throw new Error("fixture queue unavailable");
    if (!this.job) {
      this.job = job;
      this.status = {
        jobId: crypto.randomUUID(),
        task: job.task,
        state: "pending",
        attempt: 0,
        maxAttempts: job.maxAttempts,
        cancelRequested: false,
        result: null,
        error: null,
      };
    } else {
      expect(job.input).toEqual(this.job.input);
      expect(job.deduplicationKey).toBe(this.job.deduplicationKey);
    }
    return this.status!.jobId;
  }
  async get() {
    return this.status;
  }
  async cancel() {
    return "missing" as const;
  }
  async retry() {
    if (this.status?.state !== "failed") return false;
    this.retried++;
    this.status = { ...this.status, state: "pending" };
    return true;
  }
  async startWorker(execute: (job: ClaimedJob) => Promise<ExecutionResult>) {
    this.execute = execute;
    return { done: Promise.resolve(), stop: async () => {} };
  }
  async close() {}
  async run() {
    if (!this.status || !this.job || !this.execute) throw new Error("Fixture not started");
    const attempt = this.status.attempt + 1;
    const result = await this.execute({
      jobId: this.status.jobId,
      task: this.job.task,
      input: this.job.input,
      attempt,
      signal: new AbortController().signal,
    });
    this.status = {
      ...this.status,
      attempt,
      state: result.ok ? "succeeded" : "failed",
      result: result.ok ? result.result : null,
      error: result.ok ? null : result.error,
    };
    return result;
  }
}

const cleanups: (() => void | Promise<void>)[] = [];
afterEach(async () => {
  for (const close of cleanups.splice(0).reverse()) await close();
});

it("recovers persisted notification after enqueue failure without another logical record or PII payload", async () => {
  const fixture = await localFixture();
  cleanups.push(fixture.close);
  const provider = new QueueFixture();
  const task = createNotificationTask({ name: "notifications-deliver", service: fixture.service });
  const queue = createTaskQueue({ provider, tasks: [task] });
  cleanups.push(() => queue.close());
  const dispatcher = createNotificationDispatcher({ service: fixture.service, queue, task });
  provider.failEnqueue = true;
  await expect(dispatcher.submit(input)).rejects.toBeDefined();
  const outbox = await fixture.service.recoverable();
  expect(outbox).toHaveLength(1);
  provider.failEnqueue = false;
  const recovered = await dispatcher.reconcile();
  expect(recovered).toHaveLength(1);
  const duplicate = await dispatcher.submit(input);
  expect(duplicate.notification.id).toBe(outbox[0].id);
  expect(duplicate.jobId).toBe(recovered[0].jobId);
  expect(provider.job?.input).toEqual({ notificationId: outbox[0].id });
  expect(JSON.stringify(provider.job)).not.toContain(input.email);
  const worker = await queue.startWorker();
  cleanups.push(() => worker.stop());
  expect(await provider.run()).toMatchObject({ ok: true });
  expect((await fixture.service.get(outbox[0].id))?.state).toBe("accepted");
  expect(provider.status?.result).toEqual({
    notificationId: outbox[0].id,
    state: "accepted",
    attemptCount: 1,
  });
  expect(await dispatcher.reconcile()).toEqual([]);
  expect(await dispatcher.requeue(outbox[0].id)).toBe(false);
});

it("retry uses the same Tasks job; reconciliation never extends final failure attempt budget", async () => {
  let calls = 0;
  const fixture = await localFixture(() =>
    ++calls === 1
      ? Response.json({ name: "rate_limit_exceeded" }, { status: 429 })
      : Response.json({ id: "retry-id" }),
  );
  cleanups.push(fixture.close);
  const provider = new QueueFixture();
  const task = createNotificationTask({ name: "notifications-deliver", service: fixture.service });
  const queue = createTaskQueue({ provider, tasks: [task] });
  cleanups.push(() => queue.close());
  const dispatcher = createNotificationDispatcher({ service: fixture.service, queue, task });
  const submitted = await dispatcher.submit(input);
  const worker = await queue.startWorker();
  cleanups.push(() => worker.stop());
  expect(await provider.run()).toMatchObject({ ok: false });
  expect((await fixture.service.get(submitted.notification.id))?.state).toBe("failed");
  expect(await dispatcher.reconcile()).toEqual([]);
  expect(provider.retried).toBe(0);
  expect(await dispatcher.requeue(submitted.notification.id)).toBe(true);
  expect(provider.retried).toBe(1);
  expect(await provider.run()).toMatchObject({ ok: true });
  expect(provider.status?.jobId).toBe(submitted.jobId ?? undefined);
  expect(
    (await fixture.service.attempts(submitted.notification.id)).map((attempt) => attempt.number),
  ).toEqual([1, 2]);
  expect(await dispatcher.requeue(submitted.notification.id)).toBe(false);
  expect(calls).toBe(2);
});

it("recovers a committed enqueue when the handoff marker write fails without adding another job", async () => {
  const fixture = await localFixture();
  cleanups.push(fixture.close);
  let failMarker = true;
  const service = createNotificationService({
    store: {
      ...fixture.store,
      async markEnqueued(id, taskJobId) {
        if (failMarker) {
          failMarker = false;
          throw new Error("Fixture handoff write lost");
        }
        return fixture.store.markEnqueued(id, taskJobId);
      },
    },
    channels: [fixture.channel],
    templates: [template],
  });
  const provider = new QueueFixture();
  const task = createNotificationTask({ name: "notifications-deliver", service });
  const queue = createTaskQueue({ provider, tasks: [task] });
  cleanups.push(() => queue.close());
  const dispatcher = createNotificationDispatcher({ service, queue, task });
  await expect(dispatcher.submit(input)).rejects.toBeDefined();
  const originalJob = provider.status?.jobId;
  expect(originalJob).toBeDefined();
  const [unhanded] = await service.recoverable();
  expect((await fixture.store.get(unhanded.id))?.taskJobId).toBeNull();
  const repaired = await dispatcher.reconcile();
  expect(repaired[0].jobId).toBe(originalJob!);
  expect((await fixture.store.get(unhanded.id))?.taskJobId).toBe(originalJob!);
  expect(await service.recoverable()).toEqual([]);
  expect(provider.retried).toBe(0);
});

it("repairs an expired sending claim after completion persistence failure exhausts early Tasks retries", async () => {
  let now = 1_000_000;
  const keys: (string | null)[] = [];
  const fixture = await localFixture((request) => {
    keys.push(request.headers.get("idempotency-key"));
    return Response.json({ id: "same-provider-message" });
  });
  cleanups.push(fixture.close);
  let failCompletion = true;
  const service = createNotificationService({
    store: {
      ...fixture.store,
      async save(record, expected, attempt) {
        if (record.state === "accepted" && failCompletion) {
          failCompletion = false;
          throw new Error("Fixture completion write lost");
        }
        return fixture.store.save(record, expected, attempt);
      },
    },
    channels: [fixture.channel],
    templates: [template],
    clock: () => now,
  });
  const provider = new QueueFixture();
  const task = createNotificationTask({ name: "notifications-deliver", service });
  const queue = createTaskQueue({ provider, tasks: [task] });
  cleanups.push(() => queue.close());
  const dispatcher = createNotificationDispatcher({ service, queue, task });
  const submitted = await dispatcher.submit(input);
  const worker = await queue.startWorker();
  cleanups.push(() => worker.stop());
  for (let attempt = 0; attempt < 3; attempt++)
    expect(await provider.run()).toMatchObject({ ok: false });
  expect(provider.status?.attempt).toBe(3);
  expect((await service.get(submitted.notification.id))?.state).toBe("sending");
  expect(await dispatcher.requeue(submitted.notification.id)).toBe(false);
  expect(keys).toHaveLength(1);
  now += 120_001;
  expect(await dispatcher.requeue(submitted.notification.id)).toBe(true);
  expect(await provider.run()).toMatchObject({ ok: true });
  expect(provider.status?.jobId).toBe(submitted.jobId ?? undefined);
  expect((await service.get(submitted.notification.id))?.state).toBe("accepted");
  expect(
    (await service.attempts(submitted.notification.id)).map((attempt) => attempt.state),
  ).toEqual(["unknown", "accepted"]);
  expect(keys).toHaveLength(2);
  expect(keys[0]).toBe(keys[1]);
});
