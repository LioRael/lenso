import type { SubjectRef } from "@lenso/auth";
import type { JsonValue, JobStatus, TaskQueueIdentity } from "@lenso/tasks";

export interface ScheduleScope {
  readonly namespace: string;
  readonly tenantId: string;
}

export type ScheduleRule =
  | { readonly kind: "once"; readonly at: number }
  | { readonly kind: "cron"; readonly expression: string; readonly timezone: string };

export interface ScheduleDefinition {
  readonly task: string;
  readonly input: JsonValue;
  readonly rule: ScheduleRule;
  readonly misfire: "skip" | "coalesce";
  /** Lateness within this window is an ordinary dispatch, not a misfire. */
  readonly graceMs: number;
}

export interface Schedule extends ScheduleDefinition {
  readonly id: string;
  readonly revision: number;
  readonly state: "active" | "paused" | "cancelled" | "completed";
  readonly nextAt: number | null;
  /** A durable reference from a trusted entry, not an actor or a credential. */
  readonly initiator: SubjectRef;
}

export type DispatchError =
  | "dispatch-failed"
  | "execution-denied"
  | "job-expired"
  | "dispatch-invalid";

export interface Occurrence {
  readonly id: string;
  readonly scheduleId: string;
  readonly revision: number;
  readonly scheduledAt: number;
  readonly source: "timer" | "manual";
  readonly task: string;
  readonly input: JsonValue;
  readonly initiator: SubjectRef;
  readonly state: "pending" | "enqueued" | "blocked";
  readonly jobId: string | null;
  readonly error: DispatchError | null;
  readonly leaseToken: string | null;
  readonly leaseUntil: number | null;
}

export interface OccurrenceStatus {
  readonly occurrence: Omit<Occurrence, "input" | "leaseToken" | "leaseUntil">;
  /** A crash can leave accepted work without a recorded jobId. */
  readonly acceptance: "confirmed" | "unknown";
  /** null is unknown/not retained, never evidence of successful execution. */
  readonly job: JobStatus | null;
}

/**
 * All operations are scoped. advance() atomically CASes the active schedule's
 * revision AND cursor and inserts the immutable outbox entry in that transaction.
 */
export interface ScheduleStore {
  readonly kind: "postgres" | "d1";
  /** Persistently pin a scope to one durable queue; never overwrite a different binding. */
  bind(scope: ScheduleScope, identity: TaskQueueIdentity): Promise<boolean>;
  create(scope: ScheduleScope, schedule: Schedule): Promise<void>;
  get(scope: ScheduleScope, id: string): Promise<Schedule | null>;
  list(scope: ScheduleScope, limit: number): Promise<Schedule[]>;
  replace(scope: ScheduleScope, expectedRevision: number, schedule: Schedule): Promise<boolean>;
  due(scope: ScheduleScope, now: number, limit: number): Promise<Schedule[]>;
  advance(
    scope: ScheduleScope,
    expected: Schedule,
    nextAt: number | null,
    occurrence: Occurrence | null,
  ): Promise<boolean>;
  /** Lock/recheck revision against updates/cancellation; manual keys are idempotent. */
  trigger(
    scope: ScheduleScope,
    expected: Schedule,
    occurrence: Occurrence,
  ): Promise<Occurrence | null>;
  claim(scope: ScheduleScope, now: number, leaseMs: number): Promise<Occurrence | null>;
  /** Confirm a still-live token after authorization, before beginning queue I/O. */
  renew(
    scope: ScheduleScope,
    id: string,
    leaseToken: string,
    now: number,
    leaseMs: number,
  ): Promise<boolean>;
  settle(
    scope: ScheduleScope,
    id: string,
    leaseToken: string,
    result: { jobId: string } | { error: DispatchError },
  ): Promise<boolean>;
  occurrences(scope: ScheduleScope, scheduleId: string, limit: number): Promise<Occurrence[]>;
}
