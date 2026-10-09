import type { Actor } from "@lenso/auth";
import type { StandardJSONSchemaV1, StandardSchemaV1 } from "@standard-schema/spec";
import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { boundedJson, defineOperation, safeInputSchema } from "@lenso/engine/operations";
import { defineManage } from "@lenso/manage";
import type { Task } from "@lenso/tasks";
import { z } from "zod";
import { SchedulerError } from "./errors";
import type { Scheduler } from "./index";

export interface SchedulerManageInvocation<A extends Actor = Actor> {
  /** Produced by the trusted host binding, never parsed from business JSON. */
  readonly actor: A;
  readonly signal: AbortSignal;
}

const id = z.string().uuid();
const query = z.object({ id }).strict();
const revision = z
  .object({ id, revision: z.number().int().min(1).max(Number.MAX_SAFE_INTEGER) })
  .strict();
const list = z.object({ limit: z.number().int().min(1).max(100).optional() }).strict();
const occurrences = z.object({ id, limit: z.number().int().min(1).max(100).optional() }).strict();
const trigger = z.object({ id, key: z.string().trim().min(1).max(256) }).strict();
const catalogInput = z.object({}).strict();

/** Opt-in companion. It borrows the installed scheduler and starts no tick or worker. */
export function createSchedulerManage<A extends Actor = Actor>(options: {
  readonly id: string;
  readonly scheduler: Plugin<Scheduler<A>>;
  /** The host supplies the same pre-registered tasks as the scheduler, not browser registrations. */
  readonly tasks: readonly Pick<Task, "name" | "input">[];
  /** Required for catalog disclosure; absent callbacks deliberately produce an empty catalog. */
  readonly authorizeCatalog?: (actor: A, signal: AbortSignal) => Promise<void>;
}) {
  if (
    !options.tasks.length ||
    new Set(options.tasks.map((task) => task.name)).size !== options.tasks.length
  ) {
    throw new SchedulerError("invalid-options");
  }
  const tasks = new Map(options.tasks.map((task) => [task.name, task]));
  const create = z
    .object({
      task: z.enum(options.tasks.map((task) => task.name)),
      input: z.json(),
      rule: z.discriminatedUnion("kind", [
        z
          .object({
            kind: z.literal("once"),
            at: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER),
          })
          .strict(),
        z
          .object({
            kind: z.literal("cron"),
            expression: z.string().min(1).max(256),
            timezone: z.string().min(1).max(256),
          })
          .strict(),
      ]),
      misfire: z.enum(["skip", "coalesce"]),
      graceMs: z.number().int().min(0).max(86_400_000),
    })
    .strict()
    .superRefine(async (value, context) => {
      try {
        const result = await tasks
          .get(value.task)!
          .input["~standard"].validate(structuredClone(value.input));
        if (!result.issues) return;
      } catch {}
      context.addIssue({ code: "custom", message: "Task input is invalid", path: ["input"] });
    });
  const source = { file: "packages/scheduler/src/manage.ts", export: "createSchedulerManage" };
  const plugin = definePlugin({
    id: options.id,
    source,
    requires: [options.scheduler],
    setup(context) {
      const service = context.get(options.scheduler);
      async function invoke<T>(invocation: SchedulerManageInvocation<A>, call: () => Promise<T>) {
        invocation.signal.throwIfAborted();
        const result = await call();
        invocation.signal.throwIfAborted();
        return result;
      }
      return {
        create: (input: z.infer<typeof create>, invocation: SchedulerManageInvocation<A>) =>
          invoke(invocation, () => service.create(input, invocation.actor)),
        list: (input: z.infer<typeof list>, invocation: SchedulerManageInvocation<A>) =>
          invoke(invocation, () => service.list(invocation.actor, input.limit)),
        get: (input: z.infer<typeof query>, invocation: SchedulerManageInvocation<A>) =>
          invoke(invocation, () => service.get(input.id, invocation.actor)),
        pause: (input: z.infer<typeof revision>, invocation: SchedulerManageInvocation<A>) =>
          invoke(invocation, () => service.pause(input.id, input.revision, invocation.actor)),
        resume: (input: z.infer<typeof revision>, invocation: SchedulerManageInvocation<A>) =>
          invoke(invocation, () => service.resume(input.id, input.revision, invocation.actor)),
        cancel: (input: z.infer<typeof revision>, invocation: SchedulerManageInvocation<A>) =>
          invoke(invocation, () => service.cancel(input.id, input.revision, invocation.actor)),
        trigger: (input: z.infer<typeof trigger>, invocation: SchedulerManageInvocation<A>) =>
          invoke(invocation, () => service.trigger(input.id, input.key, invocation.actor)),
        occurrences: (
          input: z.infer<typeof occurrences>,
          invocation: SchedulerManageInvocation<A>,
        ) =>
          invoke(invocation, async () =>
            (await service.occurrences(input.id, invocation.actor, input.limit)).map((entry) => ({
              ...entry,
              job:
                entry.job === null
                  ? null
                  : {
                      jobId: entry.job.jobId,
                      task: entry.job.task,
                      state: entry.job.state,
                      attempt: entry.job.attempt,
                      maxAttempts: entry.job.maxAttempts,
                      cancelRequested: entry.job.cancelRequested,
                    },
            })),
          ),
        catalog: (_input: z.infer<typeof catalogInput>, invocation: SchedulerManageInvocation<A>) =>
          invoke(invocation, async () => {
            if (!options.authorizeCatalog) return { tasks: [] };
            await options.authorizeCatalog(invocation.actor, invocation.signal);
            invocation.signal.throwIfAborted();
            const catalog = options.tasks.map((task) => {
              const standard: StandardSchemaV1.Props & Partial<StandardJSONSchemaV1.Props> =
                task.input["~standard"];
              let inputSchema: Record<string, unknown> | null = null;
              try {
                const converted = standard.jsonSchema?.input({ target: "draft-2020-12" });
                inputSchema = converted ? safeInputSchema(converted) : null;
              } catch {}
              return {
                name: task.name,
                schemaAvailability: inputSchema
                  ? ("available" as const)
                  : ("runtime-validation-only" as const),
                inputSchema,
              };
            });
            boundedJson(catalog, 64 * 1024);
            return { tasks: catalog };
          }),
      };
    },
  });
  const metadata = {
    plugin,
    source,
    context: true as const,
    cancellation: "none" as const,
    mapError(error: unknown) {
      if (!(error instanceof SchedulerError)) return undefined;
      return { code: error.code, phase: "invoke", message: `Scheduler ${error.code}` } as const;
    },
  };
  const operations = [
    defineOperation({
      ...metadata,
      method: "create",
      input: create,
      effect: "write",
      retry: "unsafe",
      description: "Create an authorized schedule for a pre-registered task.",
    }),
    defineOperation({
      ...metadata,
      method: "list",
      input: list,
      effect: "read",
      retry: "safe",
      description: "List authorized schedule summaries without input.",
    }),
    defineOperation({
      ...metadata,
      method: "get",
      input: query,
      effect: "read",
      retry: "safe",
      description: "Read an authorized schedule summary without input.",
    }),
    defineOperation({
      ...metadata,
      method: "pause",
      input: revision,
      effect: "write",
      retry: "unsafe",
      description: "Pause a schedule at its expected revision.",
    }),
    defineOperation({
      ...metadata,
      method: "resume",
      input: revision,
      effect: "write",
      retry: "unsafe",
      description: "Resume a schedule at its expected revision.",
    }),
    defineOperation({
      ...metadata,
      method: "cancel",
      input: revision,
      effect: "write",
      retry: "unsafe",
      destructive: true,
      description: "Cancel future scheduling at the expected revision.",
      outputDescription: "Does not cancel accepted jobs or roll back external effects.",
    }),
    defineOperation({
      ...metadata,
      method: "trigger",
      input: trigger,
      effect: "write",
      retry: "safe",
      description: "Reserve an authorized manual occurrence with an idempotency key.",
      outputDescription: "An occurrence reservation is not evidence of execution.",
    }),
    defineOperation({
      ...metadata,
      method: "occurrences",
      input: occurrences,
      effect: "read",
      retry: "safe",
      description: "Read authorized occurrences without input or job results.",
    }),
    defineOperation({
      ...metadata,
      method: "catalog",
      input: catalogInput,
      effect: "read",
      retry: "safe",
      description: "Read schemas for explicitly registered tasks after host authorization.",
    }),
  ];
  return { plugin, operations, manage: defineManage({ plugin, operations }) };
}
