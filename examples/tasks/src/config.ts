import { definePluginConfig, resolveConfig } from "@lenso/core/config";
import { envSource } from "@lenso/core/config/env";
import { z } from "zod";

export const tasksConfig = definePluginConfig({
  description: "Durable report task database and queue",
  fields: [{ path: ["connectionString"], sensitive: true }],
  schema: z.strictObject({
    connectionString: z.string().trim().min(1),
    queueName: z.string().min(1).default("reports"),
  }),
});

export type TasksConfig = z.output<typeof tasksConfig.schema>;

export function tasksEnvSource(
  read: (name: string) => string | undefined = (name) => process.env[name],
) {
  return envSource({
    id: "tasks-env",
    read,
    bindings: {
      connectionString: { name: "DATABASE_URL", sensitive: true, empty: "error" },
      queueName: { name: "TASK_QUEUE_NAME" },
    },
  });
}

export async function readConfig(): Promise<TasksConfig> {
  return (await resolveConfig("tasks", { contract: tasksConfig, sources: [tasksEnvSource()] }))
    .value;
}

export function reportFailure(operation: string): void {
  console.error(`${operation} failed. Check configuration, migrations and database availability.`);
  process.exitCode = 1;
}
