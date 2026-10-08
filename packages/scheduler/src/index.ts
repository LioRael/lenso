import type { Actor, SubjectRef } from "@lenso/auth";
import type { Logger } from "@lenso/core";
import type { StandardSchemaV1 } from "@standard-schema/spec";
import type { JsonValue, Task, TaskQueue } from "@lenso/tasks";
import type {
  Occurrence,
  OccurrenceStatus,
  Schedule,
  ScheduleDefinition,
  ScheduleScope,
  ScheduleStore,
} from "./contracts";
import { SchedulerError } from "./errors";
import { nextTrigger, planDue, timestamp } from "./rules";

export type * from "./contracts";
export { SchedulerError } from "./errors";
export { nextTrigger } from "./rules";

export type ScheduleAction =
  | "create"
  | "update"
  | "pause"
  | "resume"
  | "cancel"
  | "read"
  | "trigger";
export type ScheduleSummary = Omit<Schedule, "input">;

export interface SchedulerOptions<A extends Actor = Actor> {
  readonly store: ScheduleStore;
  readonly scope: ScheduleScope;
  /** This queue must implement the documented persistent deduplication contract. */
  readonly queue: Pick<TaskQueue, "enqueue" | "get" | "identity" | "lookupDeduplicationKey">;
  readonly tasks: readonly Task<any, any>[];
  readonly clock?: () => number;
  readonly maxSchedulesPerTick?: number;
  readonly maxDispatchesPerTick?: number;
  readonly dispatchLeaseMs?: number;
  readonly logger?: Logger;
  /** The caller obtains this branded Actor from Auth, never from business JSON. */
  readonly authorize: (
    actor: A,
    action: ScheduleAction,
    scope: ScheduleScope,
    schedule: Schedule | ScheduleDefinition | null,
  ) => boolean | Promise<boolean>;
  /** Recheck durable identity/tenant/task permission, including revocation, at dispatch. */
  readonly authorizeExecution: (
    initiator: SubjectRef,
    scope: ScheduleScope,
    occurrence: Occurrence,
  ) => boolean | Promise<boolean>;
}

function bounded(value: number, max: number) {
  if (!Number.isSafeInteger(value) || value < 1 || value > max)
    throw new SchedulerError("invalid-options");
  return value;
}

function identifier(value: string) {
  if (typeof value !== "string" || !value.trim() || value.length > 256)
    throw new SchedulerError("invalid-input");
  return value;
}

function scheduleId(value: string) {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value))
    throw new SchedulerError("invalid-input");
  return value;
}

function jsonSnapshot(input: unknown): JsonValue {
  const seen = new Set<object>();
  let nodes = 0;
  function check(value: unknown, depth: number): void {
    if (depth > 32 || ++nodes > 65_536) throw new SchedulerError("invalid-input");
    if (value === null || typeof value === "string" || typeof value === "boolean") return;
    if (typeof value === "number" && Number.isFinite(value)) return;
    if (typeof value !== "object" || seen.has(value!)) throw new SchedulerError("invalid-input");
    const object = value!;
    if (!Array.isArray(object) && ![Object.prototype, null].includes(Object.getPrototypeOf(object)))
      throw new SchedulerError("invalid-input");
    seen.add(object);
    const keys = Reflect.ownKeys(object).filter(
      (key) => key !== "length" || !Array.isArray(object),
    );
    if (Array.isArray(object) && keys.length !== object.length)
      throw new SchedulerError("invalid-input");
    for (const key of keys) {
      const descriptor = Object.getOwnPropertyDescriptor(object, key)!;
      if (
        Array.isArray(object) &&
        (typeof key !== "string" || !/^(0|[1-9]\d*)$/.test(key) || Number(key) >= object.length)
      )
        throw new SchedulerError("invalid-input");
      if (typeof key !== "string" || !descriptor.enumerable || !("value" in descriptor))
        throw new SchedulerError("invalid-input");
      check(descriptor.value, depth + 1);
    }
    seen.delete(object);
  }
  check(input, 0);
  const encoded = JSON.stringify(input);
  if (new TextEncoder().encode(encoded).byteLength > 65_536)
    throw new SchedulerError("invalid-input");
  return JSON.parse(encoded);
}

function summary(schedule: Schedule): ScheduleSummary {
  const { input: _input, ...safe } = schedule;
  return safe;
}

async function occurrenceId(parts: unknown[]): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(JSON.stringify(parts)),
  );
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function createScheduler<A extends Actor = Actor>(options: SchedulerOptions<A>) {
  if (!["postgres", "d1"].includes(options.store.kind))
    throw new SchedulerError("unsupported-storage");
  const scope = Object.freeze({
    namespace: identifier(options.scope.namespace),
    tenantId: identifier(options.scope.tenantId),
  });
  const clock = options.clock ?? Date.now;
  const scheduleLimit = bounded(options.maxSchedulesPerTick ?? 100, 1000);
  const dispatchLimit = bounded(options.maxDispatchesPerTick ?? 100, 1000);
  const leaseMs = bounded(options.dispatchLeaseMs ?? 30_000, 2_147_483_647);
  const tasks = new Map(options.tasks.map((task) => [task.name, task]));
  if (
    tasks.size !== options.tasks.length ||
    typeof options.authorize !== "function" ||
    typeof options.authorizeExecution !== "function"
  )
    throw new SchedulerError("invalid-options");

  let binding: Promise<void> | undefined;
  async function ready() {
    binding ??= (async () => {
      const identity = await options.queue.identity();
      if (identity.kind !== options.store.kind) throw new SchedulerError("unsupported-storage");
      if (!(await options.store.bind(scope, identity))) throw new SchedulerError("queue-mismatch");
    })().catch((error) => {
      binding = undefined;
      throw error;
    });
    await binding;
  }

  async function authorize(
    actor: A,
    action: ScheduleAction,
    schedule: Schedule | ScheduleDefinition | null,
  ) {
    if (!(await options.authorize(actor, action, scope, schedule)))
      throw new SchedulerError("forbidden");
  }
  async function load(id: string) {
    await ready();
    const schedule = await options.store.get(scope, scheduleId(id));
    if (!schedule) throw new SchedulerError("not-found");
    return schedule;
  }
  async function definition(input: ScheduleDefinition): Promise<ScheduleDefinition> {
    const task = tasks.get(input.task);
    if (!task || !["skip", "coalesce"].includes(input.misfire))
      throw new SchedulerError("invalid-input");
    if (!Number.isSafeInteger(input.graceMs) || input.graceMs < 0 || input.graceMs > 86_400_000)
      throw new SchedulerError("invalid-input");
    const raw = jsonSnapshot(input.input);
    try {
      const validation = await task.input["~standard"].validate(jsonSnapshot(raw));
      if (validation.issues) throw new SchedulerError("invalid-input");
      jsonSnapshot(validation.value);
    } catch {
      throw new SchedulerError("invalid-input");
    }
    nextTrigger(input.rule, timestamp(clock()));
    return {
      task: task.name,
      input: raw,
      rule:
        input.rule.kind === "once"
          ? { kind: "once", at: timestamp(input.rule.at) }
          : { kind: "cron", expression: input.rule.expression, timezone: input.rule.timezone },
      misfire: input.misfire,
      graceMs: input.graceMs,
    };
  }
  async function occurrence(
    schedule: Schedule,
    at: number,
    source: "timer" | "manual",
    key?: string,
  ): Promise<Occurrence> {
    const id = await occurrenceId([
      scope.namespace,
      scope.tenantId,
      schedule.id,
      source,
      source === "manual" ? key : [schedule.revision, at],
    ]);
    return {
      id,
      scheduleId: schedule.id,
      revision: schedule.revision,
      scheduledAt: at,
      source,
      task: schedule.task,
      input: schedule.input,
      initiator: schedule.initiator,
      state: "pending",
      jobId: null,
      error: null,
      leaseToken: null,
      leaseUntil: null,
    };
  }
  async function change(
    id: string,
    expectedRevision: number,
    actor: A,
    action: "pause" | "resume" | "cancel",
  ) {
    const current = await load(id);
    await authorize(actor, action, current);
    if (current.revision !== expectedRevision) throw new SchedulerError("conflict");
    if (current.state === "cancelled") {
      if (action === "cancel") return summary(current);
      throw new SchedulerError("cancelled");
    }
    const state = action === "pause" ? "paused" : action === "cancel" ? "cancelled" : "active";
    if (current.state === "completed" && action !== "cancel") throw new SchedulerError("conflict");
    const updated: Schedule = { ...current, state, revision: current.revision + 1 };
    if (!(await options.store.replace(scope, expectedRevision, updated)))
      throw new SchedulerError("conflict");
    return summary(updated);
  }
  return {
    async create(input: ScheduleDefinition, actor: A): Promise<ScheduleSummary> {
      const validated = await definition(input);
      await authorize(actor, "create", validated);
      await ready();
      const now = timestamp(clock());
      const schedule: Schedule = {
        ...validated,
        id: crypto.randomUUID(),
        revision: 1,
        state: "active",
        nextAt:
          validated.rule.kind === "once"
            ? timestamp(validated.rule.at)
            : nextTrigger(validated.rule, now),
        initiator: { realmId: identifier(actor.realmId), subjectId: identifier(actor.subjectId) },
      };
      await options.store.create(scope, schedule);
      return summary(schedule);
    },
    async update(id: string, expectedRevision: number, input: ScheduleDefinition, actor: A) {
      const current = await load(id);
      await authorize(actor, "update", current);
      if (current.state === "cancelled") throw new SchedulerError("cancelled");
      if (current.revision !== expectedRevision) throw new SchedulerError("conflict");
      const validated = await definition(input);
      // Authorize the proposed target too; permission on the old task is insufficient.
      await authorize(actor, "update", { ...current, ...validated });
      const updated: Schedule = {
        ...current,
        ...validated,
        revision: current.revision + 1,
        state: current.state === "paused" ? "paused" : "active",
        nextAt:
          validated.rule.kind === "once"
            ? validated.rule.at
            : nextTrigger(validated.rule, timestamp(clock())),
      };
      if (!(await options.store.replace(scope, expectedRevision, updated)))
        throw new SchedulerError("conflict");
      return summary(updated);
    },
    pause: (id: string, revision: number, actor: A) => change(id, revision, actor, "pause"),
    resume: (id: string, revision: number, actor: A) => change(id, revision, actor, "resume"),
    cancel: (id: string, revision: number, actor: A) => change(id, revision, actor, "cancel"),
    async get(id: string, actor: A) {
      const schedule = await load(id);
      await authorize(actor, "read", schedule);
      return summary(schedule);
    },
    async list(actor: A, limit = 100) {
      await authorize(actor, "read", null);
      await ready();
      const schedules = await options.store.list(scope, bounded(limit, 1000));
      const result: ScheduleSummary[] = [];
      for (const schedule of schedules)
        if (await options.authorize(actor, "read", scope, schedule)) result.push(summary(schedule));
      return result;
    },
    async trigger(id: string, key: string, actor: A) {
      const schedule = await load(id);
      await authorize(actor, "trigger", schedule);
      if (schedule.state === "cancelled") throw new SchedulerError("cancelled");
      const entry = await occurrence(schedule, timestamp(clock()), "manual", identifier(key));
      const reserved = await options.store.trigger(scope, schedule, {
        ...entry,
        initiator: { realmId: identifier(actor.realmId), subjectId: identifier(actor.subjectId) },
      });
      if (!reserved) throw new SchedulerError("conflict");
      return { occurrenceId: reserved.id };
    },
    async occurrences(id: string, actor: A, limit = 100): Promise<OccurrenceStatus[]> {
      const schedule = await load(id);
      await authorize(actor, "read", schedule);
      const result: OccurrenceStatus[] = [];
      for (const entry of await options.store.occurrences(scope, id, bounded(limit, 1000))) {
        const { input: _input, leaseToken: _token, leaseUntil: _until, ...safe } = entry;
        // A late in-flight enqueue may finish after another dispatcher blocked this entry.
        const accepted = entry.jobId
          ? null
          : await options.queue.lookupDeduplicationKey(`scheduler:${entry.id}`);
        const jobId = entry.jobId ?? accepted?.jobId ?? null;
        result.push({
          occurrence: { ...safe, jobId },
          acceptance: jobId ? "confirmed" : "unknown",
          job: entry.jobId ? await options.queue.get(entry.jobId) : (accepted?.status ?? null),
        });
      }
      return result;
    },
    /** Trusted host-only entry. No handler is executed here, only queue enqueue. */
    async tick() {
      await ready();
      const now = timestamp(clock());
      const result = { advanced: 0, enqueued: 0, denied: 0, failed: 0 };
      for (const schedule of await options.store.due(scope, now, scheduleLimit)) {
        const plan = planDue(schedule, now);
        if (!plan) continue;
        const entry =
          plan.scheduledAt === null ? null : await occurrence(schedule, plan.scheduledAt, "timer");
        if (await options.store.advance(scope, schedule, plan.nextAt, entry)) result.advanced++;
      }
      for (let count = 0; count < dispatchLimit; count++) {
        const entry = await options.store.claim(scope, timestamp(clock()), leaseMs);
        if (!entry) break;
        try {
          // Reading existing acceptance does not initiate work or require enqueue permission.
          const accepted = await options.queue.lookupDeduplicationKey(`scheduler:${entry.id}`);
          if (accepted) {
            if (
              await options.store.settle(scope, entry.id, entry.leaseToken!, {
                jobId: accepted.jobId,
              })
            )
              result.enqueued++;
            continue;
          }
        } catch {
          await options.store.settle(scope, entry.id, entry.leaseToken!, {
            error: "dispatch-failed",
          });
          result.failed++;
          continue;
        }
        let permitted: boolean;
        try {
          permitted = await options.authorizeExecution(entry.initiator, scope, entry);
        } catch {
          await options.store.settle(scope, entry.id, entry.leaseToken!, {
            error: "dispatch-failed",
          });
          result.failed++;
          continue;
        }
        if (!permitted) {
          await options.store.settle(scope, entry.id, entry.leaseToken!, {
            error: "execution-denied",
          });
          result.denied++;
          continue;
        }
        if (
          !(await options.store.renew(
            scope,
            entry.id,
            entry.leaseToken!,
            timestamp(clock()),
            leaseMs,
          ))
        )
          continue;
        try {
          const task = tasks.get(entry.task);
          if (!task) throw new SchedulerError("invalid-input");
          const jobId = await options.queue.enqueue(
            task,
            entry.input as StandardSchemaV1.InferInput<typeof task.input>,
            { deduplicationKey: `scheduler:${entry.id}` },
          );
          if (await options.store.settle(scope, entry.id, entry.leaseToken!, { jobId }))
            result.enqueued++;
        } catch (error) {
          // Permanent failures preserve the tombstone; transient failures retry the same key.
          const code =
            typeof error === "object" && error !== null && "code" in error ? error.code : undefined;
          const failure =
            code === "job-expired"
              ? "job-expired"
              : [
                    "deduplication-conflict",
                    "invalid-task",
                    "invalid-input",
                    "invalid-options",
                  ].includes(String(code))
                ? "dispatch-invalid"
                : "dispatch-failed";
          await options.store.settle(scope, entry.id, entry.leaseToken!, { error: failure });
          result.failed++;
        }
      }
      try {
        options.logger?.info(result, "Scheduler tick completed");
      } catch {}
      return result;
    },
  };
}

export type Scheduler<A extends Actor = Actor> = ReturnType<typeof createScheduler<A>>;
