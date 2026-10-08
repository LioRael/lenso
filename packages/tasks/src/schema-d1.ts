import { sql } from "drizzle-orm";
import {
  check,
  index,
  integer,
  primaryKey,
  sqliteTable,
  text,
  unique,
} from "drizzle-orm/sqlite-core";
import type { ErrorCode, JobState, JsonValue } from "./contracts";
import type { TraceMetadata } from "./telemetry";

const queue = sqliteTable("lenso_d1_task_queue", {
  queueName: text("queue_name").primaryKey().notNull(),
  queueId: text("queue_id").notNull().unique(),
});

const job = sqliteTable(
  "lenso_d1_task_job",
  {
    queueName: text("queue_name")
      .notNull()
      .references(() => queue.queueName),
    id: text("id").notNull(),
    task: text("task").notNull(),
    input: text("input", { mode: "json" }).$type<JsonValue>().notNull(),
    traceMetadata: text("trace_metadata", { mode: "json" }).$type<TraceMetadata>(),
    deduplicationKey: text("dedup_key"),
    state: text("state").$type<JobState>().notNull().default("pending"),
    attempt: integer("attempt").notNull().default(0),
    maxAttempts: integer("max_attempts").notNull(),
    cancelRequested: integer("cancel_requested", { mode: "boolean" }).notNull().default(false),
    runAt: integer("run_at").notNull(),
    leaseUntil: integer("lease_until"),
    expiresAt: integer("expires_at"),
    retryDelaySeconds: integer("retry_delay_seconds").notNull().default(0),
    retryBackoff: integer("retry_backoff", { mode: "boolean" }).notNull().default(false),
    retryMaxDelaySeconds: integer("retry_max_delay_seconds"),
    result: text("result", { mode: "json" }).$type<JsonValue>(),
    error: text("error").$type<ErrorCode>(),
  },
  (table) => [
    primaryKey({ columns: [table.queueName, table.id] }),
    unique().on(table.queueName, table.deduplicationKey),
    index("lenso_d1_task_job_due").on(table.queueName, table.state, table.runAt),
    index("lenso_d1_task_job_lease").on(table.queueName, table.state, table.leaseUntil),
    check("input_json", sql`json_valid(${table.input})`),
    check("trace_json", sql`${table.traceMetadata} IS NULL OR json_valid(${table.traceMetadata})`),
    check(
      "state",
      sql`${table.state} IN ('pending', 'running', 'succeeded', 'failed', 'cancelled')`,
    ),
    check("attempt", sql`${table.attempt} >= 0`),
    check("max_attempts", sql`${table.maxAttempts} >= 1`),
    check("cancel_requested", sql`${table.cancelRequested} IN (0, 1)`),
    check("retry_delay_seconds", sql`${table.retryDelaySeconds} >= 0`),
    check("retry_backoff", sql`${table.retryBackoff} IN (0, 1)`),
    check("retry_max_delay_seconds", sql`${table.retryMaxDelaySeconds} >= 0`),
    check("result_json", sql`${table.result} IS NULL OR json_valid(${table.result})`),
    check(
      "error",
      sql`${table.error} IS NULL OR ${table.error} IN ('handler-failed', 'invalid-input', 'invalid-result', 'aborted')`,
    ),
  ],
);

export const taskD1Schema = { queue, job };
