import { boolean, jsonb, pgSchema, primaryKey, text, unique, uuid } from "drizzle-orm/pg-core";
import type { JsonValue } from "./contracts";
import type { TraceMetadata } from "./telemetry";

export function taskQueueSchema(schema: string) {
  return pgSchema(schema).table(
    "lenso_task_relation",
    {
      queueName: text("queue_name").notNull(),
      jobId: uuid("job_id").notNull(),
      task: text("task").notNull(),
      input: jsonb("input").$type<JsonValue>().notNull(),
      traceMetadata: jsonb("trace_metadata").$type<TraceMetadata>(),
      deduplicationKey: text("deduplication_key"),
      cancelRequested: boolean("cancel_requested").notNull().default(false),
    },
    (table) => [
      primaryKey({ columns: [table.queueName, table.jobId] }),
      unique("lenso_task_relation_queue_key").on(table.queueName, table.deduplicationKey),
    ],
  );
}
