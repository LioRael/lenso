import { authErrorDiagnostic, type Auth } from "@lenso/auth";
import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { defineOperation } from "@lenso/engine/operations";
import { taskErrorDiagnostic } from "@lenso/tasks";
import { createTaskService, type TaskServiceResources } from "./authorized-service";
import { jobInput, reportQueryInput, submitInput } from "./contracts";

/** Ordinary application composition: no connection, migration or worker starts here. */
export function createTaskRecipe<R extends string, E, S extends string>(options: {
  id: string;
  auth: Plugin<Auth<R, E, S>>;
  resources: Plugin<TaskServiceResources>;
  // Trusted entry evidence, never a subject ID supplied in the business input.
  evidence: () => E;
}) {
  const source = { file: "src/recipe.ts", export: "createTaskRecipe" };
  const plugin = definePlugin({
    id: options.id,
    source,
    requires: [options.auth, options.resources],
    setup(context) {
      return createTaskService({
        ...context.get(options.resources),
        auth: context.get(options.auth),
        evidence: options.evidence,
      });
    },
  });
  const mapError = (error: unknown) => authErrorDiagnostic(error) ?? taskErrorDiagnostic(error);
  const operations = [
    defineOperation({
      plugin,
      method: "submit",
      input: submitInput,
      mapError,
      description: "Submit a report using the supplied Auth and task queue.",
      effect: "write",
      retry: "unsafe",
      cancellation: "none",
      source,
    }),
    defineOperation({
      plugin,
      method: "query",
      input: jobInput,
      mapError,
      description: "Read an owned job's safe status from the supplied task queue.",
      effect: "read",
      retry: "safe",
      cancellation: "none",
      source,
    }),
    defineOperation({
      plugin,
      method: "report",
      input: reportQueryInput,
      mapError,
      description: "Read an owned durable report from the supplied business service.",
      effect: "read",
      retry: "safe",
      cancellation: "none",
      source,
    }),
  ];
  return { plugin, operations };
}
