import { jsonb, pgTable, text } from "drizzle-orm/pg-core";
import type { RoleGraph } from "../types";

export const authorizationRoleGraphs = pgTable("authorization_role_graphs", {
  namespace: text("namespace").primaryKey(),
  revision: text("revision").notNull(),
  graph: jsonb("graph").$type<RoleGraph>().notNull(),
});
