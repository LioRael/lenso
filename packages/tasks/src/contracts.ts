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

export interface WorkerOptions {
  /** Local concurrency per worker, not a cluster-wide quota. */
  readonly concurrency?: number;
  /** Cooperative deadline; a slot remains occupied until the handler actually settles. */
  readonly timeoutMs?: number;
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
  enqueue(job: ProviderJob): Promise<string>;
  get(jobId: string): Promise<JobStatus | null>;
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
