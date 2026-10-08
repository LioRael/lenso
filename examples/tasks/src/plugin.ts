import { AuthError, type AuthSource } from "@lenso/auth";
import { definePlugin } from "lenso/plugin";
import { CliError } from "lenso-cli";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";
import { createAuthorizedTaskService } from "./authorized-service";
import { openResources } from "./resources";

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

async function connectResources(): Promise<TaskResources> {
  const resources = await openResources();
  return { ...resources, reports: resources.service };
}

export function createTasksPlugin(options: {
  connectAuth: () => Promise<TaskAuthConnection>;
  evidence: () => string | null;
  connectResources?: () => Promise<TaskResources>;
}) {
  return definePlugin({
    id: "tasks",
    async setup(context) {
      const connection = await options.connectAuth();
      if (connection.close) context.onCleanup(() => connection.close!());
      const resources = await (options.connectResources ?? connectResources)();
      context.onCleanup(() => resources.close());
      const service = createAuthorizedTaskService({
        ...resources,
        source: connection.source,
        evidence: options.evidence,
      });
      context.onCleanup(() => service.close());
      async function boundary<T>(work: () => Promise<T>): Promise<T> {
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
      return {
        submit: (input: Parameters<typeof service.submit>[0]) =>
          boundary(() => service.submit(input)),
        query: (input: Parameters<typeof service.query>[0]) => boundary(() => service.query(input)),
        cancel: (input: Parameters<typeof service.cancel>[0]) =>
          boundary(() => service.cancel(input)),
        retry: (input: Parameters<typeof service.retry>[0]) => boundary(() => service.retry(input)),
        report: (input: Parameters<typeof service.report>[0]) =>
          boundary(() => service.report(input)),
      };
    },
  });
}

export const tasks = createTasksPlugin({
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
