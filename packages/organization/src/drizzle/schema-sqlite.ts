import { integer, sqliteTable, text } from "drizzle-orm/sqlite-core";

export const organizations = sqliteTable("organizations", {
  id: text("id").primaryKey(),
  version: integer("version").notNull(),
  state: text("state").notNull(),
});
