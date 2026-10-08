import { sql } from "drizzle-orm";
import {
  bigint,
  check,
  foreignKey,
  index,
  integer,
  jsonb,
  pgTable,
  primaryKey,
  text,
} from "drizzle-orm/pg-core";
import type { JsonValue, TaskQueueIdentity } from "@lenso/tasks";
import type { SubjectRef } from "@lenso/auth";
import type { Occurrence, Schedule } from "./contracts";

export const schedules = pgTable(
  "lenso_schedule",
  {
    namespace: text("namespace").notNull(),
    tenantId: text("tenant_id").notNull(),
    id: text("id").notNull(),
    revision: integer("revision").notNull(),
    state: text("state").$type<Schedule["state"]>().notNull(),
    task: text("task").notNull(),
    input: jsonb("input").$type<JsonValue>().notNull(),
    rule: jsonb("rule").$type<Schedule["rule"]>().notNull(),
    misfire: text("misfire").$type<Schedule["misfire"]>().notNull(),
    graceMs: bigint("grace_ms", { mode: "number" }).notNull(),
    nextAt: bigint("next_at", { mode: "number" }),
    initiator: jsonb("initiator").$type<SubjectRef>().notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.namespace, table.tenantId, table.id], name: "lenso_schedule_pk" }),
    check(
      "lenso_schedule_state_check",
      sql`${table.state} IN ('active', 'paused', 'cancelled', 'completed')`,
    ),
    check("lenso_schedule_misfire_check", sql`${table.misfire} IN ('skip', 'coalesce')`),
    check("lenso_schedule_revision_check", sql`${table.revision} >= 0`),
    check("lenso_schedule_grace_check", sql`${table.graceMs} >= 0`),
    index("lenso_schedule_due_idx")
      .on(table.namespace, table.tenantId, table.nextAt, table.id)
      .where(sql`${table.state} = 'active'`),
  ],
);

export const scheduleOccurrences = pgTable(
  "lenso_schedule_occurrence",
  {
    namespace: text("namespace").notNull(),
    tenantId: text("tenant_id").notNull(),
    id: text("id").notNull(),
    scheduleId: text("schedule_id").notNull(),
    revision: integer("revision").notNull(),
    scheduledAt: bigint("scheduled_at", { mode: "number" }).notNull(),
    source: text("source").$type<Occurrence["source"]>().notNull(),
    task: text("task").notNull(),
    input: jsonb("input").$type<JsonValue>().notNull(),
    initiator: jsonb("initiator").$type<SubjectRef>().notNull(),
    state: text("state").$type<Occurrence["state"]>().notNull(),
    jobId: text("job_id"),
    error: text("error").$type<Occurrence["error"]>(),
    leaseToken: text("lease_token"),
    leaseUntil: bigint("lease_until", { mode: "number" }),
  },
  (table) => [
    primaryKey({
      columns: [table.namespace, table.tenantId, table.id],
      name: "lenso_schedule_occurrence_pk",
    }),
    foreignKey({
      columns: [table.namespace, table.tenantId, table.scheduleId],
      foreignColumns: [schedules.namespace, schedules.tenantId, schedules.id],
      name: "lenso_schedule_occurrence_schedule_fk",
    }),
    check(
      "lenso_schedule_occurrence_state_check",
      sql`${table.state} IN ('pending', 'enqueued', 'blocked')`,
    ),
    check("lenso_schedule_occurrence_source_check", sql`${table.source} IN ('timer', 'manual')`),
    check(
      "lenso_schedule_occurrence_error_check",
      sql`${table.error} IN ('dispatch-failed', 'execution-denied', 'job-expired', 'dispatch-invalid')`,
    ),
    check("lenso_schedule_occurrence_revision_check", sql`${table.revision} >= 0`),
    check(
      "lenso_schedule_occurrence_lifecycle_check",
      sql`
    (${table.state} = 'pending' AND ${table.jobId} IS NULL AND (${table.error} IS NULL OR ${table.error} = 'dispatch-failed')) OR
    (${table.state} = 'enqueued' AND ${table.jobId} IS NOT NULL AND ${table.error} IS NULL AND ${table.leaseToken} IS NULL AND ${table.leaseUntil} IS NULL) OR
    (${table.state} = 'blocked' AND ${table.jobId} IS NULL AND ${table.error} IS NOT NULL AND ${table.error} IN ('execution-denied', 'job-expired', 'dispatch-invalid') AND ${table.leaseToken} IS NULL AND ${table.leaseUntil} IS NULL)
  `,
    ),
    index("lenso_schedule_occurrence_claim_idx")
      .on(table.namespace, table.tenantId, table.scheduledAt, table.id)
      .where(sql`${table.state} = 'pending'`),
    index("lenso_schedule_occurrence_schedule_idx").on(
      table.namespace,
      table.tenantId,
      table.scheduleId,
      table.scheduledAt,
      table.id,
    ),
  ],
);

export const scheduleQueueBindings = pgTable(
  "lenso_schedule_queue_binding",
  {
    namespace: text("namespace").notNull(),
    tenantId: text("tenant_id").notNull(),
    queueKind: text("queue_kind").$type<TaskQueueIdentity["kind"]>().notNull(),
    queueId: text("queue_id").notNull(),
  },
  (table) => [
    primaryKey({
      columns: [table.namespace, table.tenantId],
      name: "lenso_schedule_queue_binding_pk",
    }),
    check("lenso_schedule_queue_binding_kind_check", sql`${table.queueKind} IN ('postgres', 'd1')`),
  ],
);

export const schedulerSchema = { schedules, scheduleOccurrences, scheduleQueueBindings };
