import { sqliteTable, text } from "drizzle-orm/sqlite-core";
import type { RoleGraph } from "../types";

export const authorizationRoleGraphs = sqliteTable("authorization_role_graphs", {
  namespace: text("namespace").primaryKey(),
  revision: text("revision").notNull(),
  graph: text("graph", { mode: "json" }).$type<RoleGraph>().notNull(),
});
