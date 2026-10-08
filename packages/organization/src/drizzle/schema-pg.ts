import { bigint, pgTable, text } from "drizzle-orm/pg-core";

export const organizations = pgTable("organizations", {
  id: text("id").primaryKey(),
  version: bigint("version", { mode: "number" }).notNull(),
  state: text("state").notNull(),
});
