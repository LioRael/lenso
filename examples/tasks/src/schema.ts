import { doublePrecision, integer, pgTable, text, timestamp } from "drizzle-orm/pg-core";

export const reports = pgTable("task_example_reports", {
  reportId: text("report_id").primaryKey(),
  sum: doublePrecision("sum").notNull(),
  count: integer("count").notNull(),
  updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).notNull(),
});
