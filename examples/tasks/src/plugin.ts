import { AuthError, type AuthSource } from "@lenso/auth";
import { definePlugin, type PluginContext } from "@lenso/core/plugin";
import { bindConfig } from "@lenso/core/config";
import { CliError } from "@lenso/cli";
import { defineOperation } from "@lenso/engine/operations";
import { defineManage } from "@lenso/manage";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";
import { createAuthorizedTaskService } from "./authorized-service";
import { jobInput, reportQueryInput, submitInput } from "./contracts";
import { openResources } from "./resources";
import { tasksConfig, tasksEnvSource, type TasksConfig } from "./config";

export interface TaskAuthConnection {
  source: AuthSource<string | null>;
  close?: () => Promise<void>;
}

type TaskResources = Omit<
  Parameters<typeof createAuthorizedTaskService>[0],
  "source" | "evidence"
> & {
  close: () => Promise<void>;
};

async function connectResources(
  context: PluginContext,
  config: TasksConfig,
): Promise<TaskResources> {
  const resources = await openResources(config, {
    instanceId: context.instanceId,
    pluginId: "tasks",
    logger: context.logger,
  });
  return { ...resources, reports: resources.service };
}

async function cliAuthBoundary<T>(work: () => Promise<T>): Promise<T> {
  try {
    return await work();
  } catch (error) {
    if (error instanceof AuthError) {
      throw new CliError({
        code: error.code,
        phase: "invoke",
        message: new AuthError(error.code).message,
      });
    }
    throw error;
  }
}

export function createTasksPlugin(options: {
  connectAuth: () => Promise<TaskAuthConnection>;
  evidence: () => string | null;
  connectResources?: () => Promise<TaskResources>;
}) {
  const setup = async (context: PluginContext, acquire: () => Promise<TaskResources>) => {
    const connection = await options.connectAuth();
    if (connection.close) context.onCleanup(() => connection.close!());
    const resources = await acquire();
    context.onCleanup(() => resources.close());
    const service = createAuthorizedTaskService({
      ...resources,
      source: connection.source,
      evidence: options.evidence,
    });
    context.onCleanup(() => service.close());
    return {
      submit: (input: Parameters<typeof service.submit>[0]) =>
        cliAuthBoundary(() => service.submit(input)),
      query: (input: Parameters<typeof service.query>[0]) =>
        cliAuthBoundary(() => service.query(input)),
      cancel: (input: Parameters<typeof service.cancel>[0]) =>
        cliAuthBoundary(() => service.cancel(input)),
      retry: (input: Parameters<typeof service.retry>[0]) =>
        cliAuthBoundary(() => service.retry(input)),
      report: (input: Parameters<typeof service.report>[0]) =>
        cliAuthBoundary(() => service.report(input)),
    };
  };
  const injected = options.connectResources;
  return injected
    ? definePlugin({ id: "tasks", setup: (context) => setup(context, injected) })
    : bindConfig(tasksConfig, [tasksEnvSource()], {
        id: "tasks",
        setup: (context, config) => setup(context, () => connectResources(context, config)),
      });
}

export function createTasksOperations(options: Parameters<typeof createTasksPlugin>[0]) {
  const plugin = createTasksPlugin(options);
  const source = { file: "src/plugin.ts", export: "createTasksOperations" };
  const operations = [
    defineOperation({
      plugin,
      method: "submit",
      input: submitInput,
      description: "Submit a report owned by the authenticated session subject.",
      effect: "write",
      destructive: false,
      retry: "unsafe",
      cancellation: "none",
      outputDescription:
        "An owned jobId. Without a deduplication key, repeating creates another job.",
      source,
    }),
    defineOperation({
      plugin,
      method: "query",
      input: jobInput,
      description: "Read safe status after durable owner authorization.",
      effect: "read",
      destructive: false,
      retry: "safe",
      cancellation: "none",
      outputDescription:
        "State, attempt budget and cancellation request flag, or null after pruning.",
      source,
    }),
    defineOperation({
      plugin,
      method: "cancel",
      input: jobInput,
      description: "Request cancellation of an owned job; requested does not mean stopped.",
      effect: "write",
      destructive: true,
      retry: "safe",
      cancellation: "request-only",
      outputDescription:
        "cancelled, requested, terminal or missing. No external effects are rolled back.",
      source,
    }),
    defineOperation({
      plugin,
      method: "retry",
      input: jobInput,
      description: "Retry an owned final failure, preserving payload and attempt count.",
      effect: "write",
      destructive: false,
      retry: "unsafe",
      cancellation: "none",
      outputDescription: "true only if a final failure received one more attempt; otherwise false.",
      source,
    }),
    defineOperation({
      plugin,
      method: "report",
      input: reportQueryInput,
      description: "Read an owned report from the business table.",
      effect: "read",
      destructive: false,
      retry: "safe",
      cancellation: "none",
      outputDescription: "sum and count, or null before the report is written.",
      source,
    }),
  ];
  const manage = defineManage({ plugin, operations: operations.slice(0, 4) });
  return { plugin, operations, manage };
}

export const taskOperations = createTasksOperations({
  evidence: () => process.env.TASK_SESSION ?? null,
  async connectAuth() {
    const file = process.env.TASK_AUTH_SOURCE_MODULE;
    if (!file) throw new Error("Configure a trusted task authentication source.");
    const module = await import(pathToFileURL(resolve(file)).href);
    if (typeof module.connectTaskAuth !== "function")
      throw new Error("Invalid task authentication source.");
    return module.connectTaskAuth();
  },
});
export const tasks = taskOperations.plugin;
