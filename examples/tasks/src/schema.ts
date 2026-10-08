import {
  doublePrecision,
  integer,
  pgTable,
  primaryKey,
  text,
  timestamp,
} from "drizzle-orm/pg-core";

export const reports = pgTable("task_example_reports", {
  reportId: text("report_id").primaryKey(),
  sum: doublePrecision("sum").notNull(),
  count: integer("count").notNull(),
  updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).notNull(),
});

export const reportOwners = pgTable("task_example_report_owners", {
  reportId: text("report_id").primaryKey(),
  realmId: text("realm_id").notNull(),
  subjectId: text("subject_id").notNull(),
});

export const jobReports = pgTable(
  "task_example_job_reports",
  {
    queueName: text("queue_name").notNull(),
    jobId: text("job_id").notNull(),
    reportId: text("report_id")
      .notNull()
      .references(() => reportOwners.reportId),
  },
  (table) => [primaryKey({ columns: [table.queueName, table.jobId] })],
);
