import type { StandardSchemaV1 } from "@standard-schema/spec";
import type { Logger } from "@lenso/core";
import type { TraceMetadata } from "./telemetry";

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export interface TaskContext {
  readonly instanceId?: string;
  readonly pluginId?: string;
  readonly logger?: Logger;
  readonly jobId: string;
  /** Starts at 1; use a business key, not this attempt number, for side-effect idempotency. */
  readonly attempt: number;
  readonly signal: AbortSignal;
}

export interface Task<S extends StandardSchemaV1 = StandardSchemaV1, R = unknown> {
  readonly name: string;
  readonly input: S;
  readonly handler: (input: StandardSchemaV1.InferOutput<S>, context: TaskContext) => Promise<R>;
  /** Only this projection is persisted/returned. Omitting it discards the handler's return value. */
  readonly result?: (value: R) => JsonValue;
  readonly maxAttempts?: number;
  readonly retry?: {
    readonly delaySeconds?: number;
    readonly backoff?: boolean;
    readonly maxDelaySeconds?: number;
  };
}

export type JobState = "pending" | "running" | "succeeded" | "failed" | "cancelled";
export type ErrorCode = "handler-failed" | "invalid-input" | "invalid-result" | "aborted";

export interface JobStatus {
  readonly jobId: string;
  readonly task: string;
  readonly state: JobState;
  readonly attempt: number;
  readonly maxAttempts: number;
  readonly cancelRequested: boolean;
  readonly result: JsonValue | null;
  readonly error: ErrorCode | null;
}

export interface EnqueueOptions {
  readonly runAt?: Date;
  /** Scoped to this queue; retained until the job is explicitly deleted by an operator. */
  readonly deduplicationKey?: string;
}

/** Operational metadata only; no payload, result or error text. */
export type JobSummary = Pick<
  JobStatus,
  "jobId" | "task" | "state" | "attempt" | "maxAttempts" | "cancelRequested"
>;

export interface JobQuery {
  /** Explicit allowlist; an empty list matches nothing. At most 100 names. */
  readonly tasks: readonly string[];
  /** At most 100 rows. Defaults to 50. */
  readonly limit?: number;
  /** Exclusive immutable job ID cursor. Rows are ordered by ascending job ID. */
  readonly after?: string;
}

export interface JobPage {
  readonly items: readonly JobSummary[];
  readonly nextCursor: string | null;
}

export interface TaskQueueIdentity {
  readonly kind: "postgres" | "d1";
  /** Persisted UUID of the named durable queue, not a connection or process identity. */
  readonly id: string;
}

export interface DeduplicationLookup {
  readonly jobId: string;
  /** Null means the accepted job was pruned while its tombstone remains. */
  readonly status: JobStatus | null;
}

export interface WorkerOptions {
  /** Local concurrency per worker, not a cluster-wide quota. */
  readonly concurrency?: number;
  /** Cooperative deadline; a slot remains occupied until the handler actually settles. */
  readonly timeoutMs?: number;
  /** Global fetch/claim attempt budget across all local lanes, at most 1000. */
  readonly maxJobs?: number;
  /** Stop claiming after an empty fetch, then drain owned work. Defaults to false. */
  readonly stopWhenIdle?: boolean;
}

export interface TaskWorker {
  /** Resolves after drain, rejects on worker failure. Hosts should observe this promise. */
  readonly done: Promise<void>;
  /** Stops claiming first. Abort requests cancellation but still waits for handler settlement. */
  stop(options?: { abort?: boolean }): Promise<void>;
}

export interface ProviderJob {
  readonly traceMetadata?: TraceMetadata;
  readonly task: string;
  readonly input: JsonValue;
  readonly maxAttempts: number;
  readonly retry?: Task["retry"];
  readonly runAt?: Date;
  readonly deduplicationKey?: string;
}

export interface ClaimedJob extends TaskContext {
  readonly traceMetadata?: TraceMetadata;
  readonly task: string;
  readonly input: JsonValue;
}

export type ExecutionResult =
  | { readonly ok: true; readonly result: JsonValue | null }
  | { readonly ok: false; readonly error: ErrorCode };

/** A provider owns durable state, claiming, retries, cancellation signalling and worker drain. */
export interface TaskProvider {
  identity(): Promise<TaskQueueIdentity>;
  lookupDeduplicationKey(key: string): Promise<DeduplicationLookup | null>;
  enqueue(job: ProviderJob): Promise<string>;
  get(jobId: string): Promise<JobStatus | null>;
  /** Optional for existing/custom providers. Never substitute an unbounded scan. */
  list?(query: JobQuery): Promise<JobPage>;
  cancel(jobId: string): Promise<"requested" | "cancelled" | "terminal" | "missing">;
  /** Retry only a final failure. Preserves jobId, payload and business idempotency key. */
  retry(jobId: string): Promise<boolean>;
  startWorker(
    execute: (job: ClaimedJob) => Promise<ExecutionResult>,
    options?: WorkerOptions,
  ): Promise<TaskWorker>;
  /** Drains owned workers and closes only provider-owned clients. */
  close(): Promise<void>;
}
