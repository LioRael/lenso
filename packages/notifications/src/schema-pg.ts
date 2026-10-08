import {
  bigint,
  boolean,
  check,
  integer,
  jsonb,
  pgTable,
  text,
  unique,
  index,
} from "drizzle-orm/pg-core";
import { sql } from "drizzle-orm";
import type { NotificationRecord, DeliveryAttempt } from "./contracts";

const timestamp = (name: string) => bigint(name, { mode: "number" });
export const notifications = pgTable(
  "lenso_notifications",
  {
    id: text("id").primaryKey(),
    tenantId: text("tenant_id").notNull(),
    scope: text("scope").notNull(),
    idempotencyKey: text("idempotency_key").notNull(),
    recipientId: text("recipient_id").notNull(),
    snapshot: jsonb("snapshot").$type<NotificationRecord>().notNull(),
    state: text("state").$type<NotificationRecord["state"]>().notNull(),
    revision: integer("revision").notNull(),
    attemptCount: integer("attempt_count").notNull(),
    firstRequestAt: timestamp("first_request_at"),
    leaseUntil: timestamp("lease_until"),
    taskJobId: text("task_job_id"),
    providerMessageId: text("provider_message_id"),
    error: text("error").$type<NotificationRecord["error"]>(),
    retryable: boolean("retryable").notNull(),
    createdAt: timestamp("created_at").notNull(),
    updatedAt: timestamp("updated_at").notNull(),
    mutationToken: text("mutation_token"),
  },
  (t) => [
    unique("lenso_notifications_key_unique").on(t.tenantId, t.scope, t.idempotencyKey),
    check("lenso_notifications_revision_check", sql`${t.revision} >= 0 AND ${t.attemptCount} >= 0`),
    index("lenso_notifications_tenant_recipient").on(t.tenantId, t.recipientId, t.createdAt),
    index("lenso_notifications_recovery").on(t.state, t.retryable, t.leaseUntil, t.updatedAt),
  ],
);

export const notificationAttempts = pgTable(
  "lenso_notification_attempts",
  {
    id: text("id").primaryKey(),
    notificationId: text("notification_id")
      .notNull()
      .references(() => notifications.id),
    number: integer("number").notNull(),
    state: text("state").$type<DeliveryAttempt["state"]>().notNull(),
    startedAt: timestamp("started_at").notNull(),
    finishedAt: timestamp("finished_at"),
    providerMessageId: text("provider_message_id"),
    error: text("error").$type<DeliveryAttempt["error"]>(),
  },
  (t) => [
    unique("lenso_notification_attempt_number_unique").on(t.notificationId, t.number),
    check("lenso_notification_attempt_number_check", sql`${t.number} > 0`),
  ],
);

export const notificationPreferences = pgTable(
  "lenso_notification_preferences",
  {
    tenantId: text("tenant_id").notNull(),
    recipientId: text("recipient_id").notNull(),
    category: text("category").notNull(),
    channelId: text("channel_id").notNull(),
    enabled: boolean("enabled").notNull(),
  },
  (t) => [
    unique("lenso_notification_preference_unique").on(
      t.tenantId,
      t.recipientId,
      t.category,
      t.channelId,
    ),
  ],
);

export const notificationSchema = { notifications, notificationAttempts, notificationPreferences };
