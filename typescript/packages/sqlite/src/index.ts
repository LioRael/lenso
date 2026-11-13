import { randomUUID } from "node:crypto";
import type { DatabaseSync, SQLOutputValue } from "node:sqlite";
import { ActionError } from "@lenso/app";
import type { DurableEvent, EventDelivery, Task } from "@lenso/app";

export type JobState = "queued" | "running" | "succeeded" | "failed" | "cancelled";
export interface Job {
  readonly id: string;
  readonly task: string;
  readonly state: JobState;
  readonly attempts: number;
  readonly maxAttempts: number;
  readonly availableMs: number;
  readonly retryMs: number;
  readonly payload: string;
  readonly result: string | null;
  readonly error: string | null;
}

export interface EnqueueOptions {
  readonly id?: string;
  readonly delayMs?: number;
  readonly maxAttempts?: number;
  readonly retryMs?: number;
}

export type Delivery = EventDelivery;

export interface TaskAttempt {
  readonly attempt: number;
  readonly startedMs: number;
  readonly finishedMs: number | null;
  readonly outcome: "running" | "succeeded" | "failed" | "cancelled";
  readonly error: string | null;
}

function integer(value: number, min: number, max = Number.MAX_SAFE_INTEGER): void {
  if (!Number.isSafeInteger(value) || value < min || value > max) throw new ActionError("invalid_input", "Invalid task timing or limit");
}

function deadline(now: number, delay: number): number {
  integer(now, 0);
  const result = now + delay;
  integer(result, 0);
  return result;
}

function decodeJob(row: Record<string, SQLOutputValue>): Job {
  const { id, task, state, attempts, max_attempts, available_ms, retry_ms, payload, result, error } = row;
  if (typeof id !== "string" || typeof task !== "string" || typeof payload !== "string"
    || typeof attempts !== "number" || typeof max_attempts !== "number" || typeof available_ms !== "number" || typeof retry_ms !== "number"
    || (result !== null && typeof result !== "string") || (error !== null && typeof error !== "string")
    || (state !== "queued" && state !== "running" && state !== "succeeded" && state !== "failed" && state !== "cancelled")) {
    throw new ActionError("internal", "Invalid stored job");
  }
  return { id, task, state, attempts, maxAttempts: max_attempts, availableMs: available_ms, retryMs: retry_ms, payload, result, error };
}

/** Owns queue operations, not the supplied SQLite connection or a background worker loop. */
export class SqliteQueue {
  constructor(private readonly db: DatabaseSync, private readonly now = Date.now) {
    db.exec(`CREATE TABLE IF NOT EXISTS lenso_jobs (
      id TEXT PRIMARY KEY, task TEXT NOT NULL, payload TEXT NOT NULL,
      state TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, max_attempts INTEGER NOT NULL,
      available_ms INTEGER NOT NULL, retry_ms INTEGER NOT NULL,
      lease_token TEXT, lease_until INTEGER, cancel_requested INTEGER NOT NULL DEFAULT 0,
      result TEXT, error TEXT
    );
    CREATE INDEX IF NOT EXISTS lenso_jobs_ready ON lenso_jobs(task, state, available_ms);
    CREATE TABLE IF NOT EXISTS lenso_task_attempts (
      job_id TEXT NOT NULL, attempt INTEGER NOT NULL, started_ms INTEGER NOT NULL,
      finished_ms INTEGER, outcome TEXT NOT NULL, error TEXT, PRIMARY KEY(job_id, attempt)
    );`);
  }

  /** Submission belongs inside an authorized action. Reusing an ID requires the same payload and task. */
  enqueue<I, O, R>(task: Task<I, O, R>, input: NoInfer<I>, options: EnqueueOptions = {}): Job {
    let payload: string;
    try { payload = task.input.encode(input); }
    catch (cause) { throw new ActionError("invalid_input", "Invalid task payload", { cause }); }
    return this.enqueueSerialized(task.id, payload, options);
  }

  /** A single statement participates in an existing native SQLite transaction or outbox. */
  enqueueSerialized(task: string, payload: string, options: EnqueueOptions = {}): Job {
    return this.enqueueAt(task, payload, options, this.now());
  }

  private enqueueAt(task: string, payload: string, options: EnqueueOptions, now: number): Job {
    const id = options.id ?? randomUUID();
    const delay = options.delayMs ?? 0;
    const attempts = options.maxAttempts ?? 3;
    const retry = options.retryMs ?? 1000;
    integer(delay, 0); integer(attempts, 1, 0xffff_ffff); integer(retry, 0);
    const available = deadline(now, delay);
    deadline(now, retry);
    this.db.prepare(`INSERT INTO lenso_jobs (id, task, payload, state, max_attempts, available_ms, retry_ms)
      VALUES (?, ?, ?, 'queued', ?, ?, ?) ON CONFLICT(id) DO NOTHING`)
      .run(id, task, payload, attempts, available, retry);
    const job = this.get(id);
    if (job.task !== task || job.payload !== payload) throw new ActionError("conflict", "Job ID already has another payload or task");
    return job;
  }

  /** Publishes a typed event; its subscribers own the mapping to durable task inputs. */
  publish<T>(event: DurableEvent<T>, eventId: string, value: T, options: Omit<EnqueueOptions, "id"> = {}): readonly Job[] {
    return this.publishSerialized(event.id, eventId, event.deliveries(value), options);
  }

  /** Atomically persists prepared jobs, including inside a caller-owned transaction. */
  publishSerialized(event: string, eventId: string, deliveries: readonly Delivery[], options: Omit<EnqueueOptions, "id"> = {}): readonly Job[] {
    if (new Set(deliveries.map((item) => item.subscription)).size !== deliveries.length) {
      throw new ActionError("conflict", "Duplicate event subscription");
    }
    this.db.exec("SAVEPOINT lenso_publish");
    try {
      const now = this.now();
      const jobs = deliveries.map((item) => this.enqueueAt(item.task, item.payload, {
        ...options, id: JSON.stringify([event, eventId, item.subscription]),
      }, now));
      this.db.exec("RELEASE SAVEPOINT lenso_publish");
      return jobs;
    } catch (error) {
      this.db.exec("ROLLBACK TO SAVEPOINT lenso_publish; RELEASE SAVEPOINT lenso_publish");
      throw error;
    }
  }

  attempts(id: string): readonly TaskAttempt[] {
    this.get(id);
    return this.db.prepare("SELECT * FROM lenso_task_attempts WHERE job_id = ? ORDER BY attempt").all(id).map((row) => {
      const { attempt, started_ms, finished_ms, outcome, error } = row;
      if (typeof attempt !== "number" || typeof started_ms !== "number" || (finished_ms !== null && typeof finished_ms !== "number")
        || (error !== null && typeof error !== "string")
        || (outcome !== "running" && outcome !== "succeeded" && outcome !== "failed" && outcome !== "cancelled")) {
        throw new ActionError("internal", "Invalid stored task attempt");
      }
      return { attempt, startedMs: started_ms, finishedMs: finished_ms, outcome, error };
    });
  }

  get(id: string): Job {
    const row = this.db.prepare("SELECT * FROM lenso_jobs WHERE id = ?").get(id);
    if (!row) throw new ActionError("not_found", "Job not found");
    return decodeJob(row);
  }

  /** Queued jobs stop immediately; running handlers observe cancellation cooperatively. */
  cancel(id: string): boolean {
    const result = this.db.prepare(`UPDATE lenso_jobs SET cancel_requested = 1,
      state = CASE WHEN state = 'queued' THEN 'cancelled' ELSE state END
      WHERE id = ? AND state IN ('queued', 'running')`).run(id);
    return result.changes > 0;
  }

  /** Claims one job, renews its lease, awaits the handler, and persists its terminal or retry state. */
  async runOne<I, O, R>(task: Task<I, O, R>, resources: NoInfer<R>, options: { leaseMs?: number; concurrency?: number } = {}): Promise<Job | undefined> {
    const leaseMs = options.leaseMs ?? 30_000;
    const concurrency = options.concurrency ?? 1;
    integer(leaseMs, 3); integer(concurrency, 1, 0xffff_ffff);
    const claim = this.claim(task.id, leaseMs, concurrency);
    if (!claim) return undefined;
    const controller = new AbortController();
    let leaseError: unknown;
    const timer = setInterval(() => {
      try {
        const now = this.now();
        const row = this.db.prepare(`UPDATE lenso_jobs SET lease_until = ?
          WHERE id = ? AND state = 'running' AND lease_token = ? AND lease_until > ? RETURNING cancel_requested`)
          .get(deadline(now, leaseMs), claim.job.id, claim.token, now);
        if (!row) throw new ActionError("conflict", "Task lease lost");
        if (row.cancel_requested === 1) controller.abort();
      } catch (error) { leaseError = error; controller.abort(); }
    }, Math.min(0x7fff_ffff, Math.floor(leaseMs / 3)));
    let result: string | null = null;
    let failure: ActionError | undefined;
    try {
      let input: I;
      try { input = task.input.decode(claim.job.payload); }
      catch (cause) { throw new ActionError("invalid_input", "Invalid task payload", { cause }); }
      const output = await task.run({ jobId: claim.job.id, attempt: claim.job.attempts, resources, signal: controller.signal }, input);
      try { result = task.output.encode(output); }
      catch (cause) { throw new ActionError("internal", "Invalid output", { cause }); }
    } catch (error) {
      failure = error instanceof ActionError ? error : new ActionError("internal", "Task failed", { cause: error });
    } finally { clearInterval(timer); }
    if (leaseError) throw leaseError;
    const state = !failure ? "succeeded" : failure.code === "cancelled" ? "cancelled"
      : claim.job.attempts >= claim.job.maxAttempts ? "failed" : "queued";
    return this.transaction(() => {
      const now = this.now();
      const available = state === "queued" ? deadline(now, claim.job.retryMs) : claim.job.availableMs;
      const row = this.db.prepare(`UPDATE lenso_jobs SET state = CASE WHEN cancel_requested = 1 AND ? <> 'succeeded' THEN 'cancelled' ELSE ? END, result = ?, error = ?,
        available_ms = ?, lease_token = NULL, lease_until = NULL
        WHERE id = ? AND state = 'running' AND lease_token = ? AND lease_until > ? RETURNING *`)
        .get(state, state, result, failure?.message ?? null, available, claim.job.id, claim.token, now);
      if (!row) throw new ActionError("conflict", "Task lease lost");
      const job = decodeJob(row);
      this.db.prepare("UPDATE lenso_task_attempts SET finished_ms = ?, outcome = ?, error = ? WHERE job_id = ? AND attempt = ?")
        .run(now, job.state === "queued" ? "failed" : job.state, failure?.message ?? null, job.id, job.attempts);
      return job;
    });
  }

  private claim(task: string, leaseMs: number, concurrency: number) {
    return this.transaction(() => {
      const now = this.now();
      const until = deadline(now, leaseMs);
      this.db.prepare(`UPDATE lenso_task_attempts SET finished_ms = ?, outcome = CASE WHEN (
        SELECT cancel_requested FROM lenso_jobs WHERE id = job_id) = 1 THEN 'cancelled' ELSE 'failed' END,
        error = 'Task lease expired' WHERE outcome = 'running' AND job_id IN (
          SELECT id FROM lenso_jobs WHERE task = ? AND state = 'running' AND lease_until <= ?
        )`).run(now, task, now);
      this.db.prepare(`UPDATE lenso_jobs SET state = CASE WHEN cancel_requested = 1 THEN 'cancelled'
        WHEN attempts >= max_attempts THEN 'failed' ELSE 'queued' END,
        lease_token = NULL, lease_until = NULL, error = 'Task lease expired'
        WHERE task = ? AND state = 'running' AND lease_until <= ?`).run(task, now);
      const token = randomUUID();
      const row = this.db.prepare(`UPDATE lenso_jobs SET state = 'running', attempts = attempts + 1,
        lease_token = ?, lease_until = ? WHERE id = (
          SELECT id FROM lenso_jobs WHERE task = ? AND state = 'queued' AND available_ms <= ?
          AND (SELECT count(*) FROM lenso_jobs WHERE task = ? AND state = 'running') < ?
          ORDER BY available_ms, id LIMIT 1
        ) RETURNING *`).get(token, until, task, now, task, concurrency);
      if (!row) return undefined;
      const job = decodeJob(row);
      this.db.prepare("INSERT INTO lenso_task_attempts (job_id, attempt, started_ms, outcome) VALUES (?, ?, ?, 'running')")
        .run(job.id, job.attempts, now);
      return { job, token };
    });
  }

  private transaction<T>(run: () => T): T {
    this.db.exec("BEGIN IMMEDIATE");
    try {
      const result = run();
      this.db.exec("COMMIT");
      return result;
    } catch (error) { this.db.exec("ROLLBACK"); throw error; }
  }
}
