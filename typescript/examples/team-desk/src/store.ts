import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { ActionError, requirePrincipal } from "@lenso/app";
import type { Context, CallRecord } from "@lenso/app";
import { SqliteQueue } from "@lenso/sqlite";

import { role, projectSchema, itemSchema } from "./contracts.js";
import type { Identity, Item } from "./contracts.js";

export class Store {
  readonly db: DatabaseSync;
  readonly queue: SqliteQueue;
  constructor(filename: string, readonly identities: readonly Identity[]) {
    if (new Set(identities.map((value) => value.token)).size !== identities.length) throw new Error("Duplicate identity token");
    this.db = new DatabaseSync(filename);
    this.db.exec(readFileSync(new URL("../../../../examples/team-desk/schema.sql", import.meta.url), "utf8"));
    for (const identity of identities) this.db.prepare("INSERT INTO desk_members(team,subject,role) VALUES(?,?,?) ON CONFLICT DO NOTHING").run(identity.team, identity.subject, identity.role);
    this.queue = new SqliteQueue(this.db);
  }
  team(context: Context<Store>): string {
    const principal = requirePrincipal(context.principal);
    const identity = this.identities.find((value) => value.subject === principal.subject && principal.scopes.includes(`team:${value.team}`));
    if (!identity) throw new ActionError("unauthenticated", "Unknown identity");
    return identity.team;
  }
  context(token: string | undefined, callId: string, signal: AbortSignal): Context<Store> {
    const identity = this.identities.find((value) => value.token === token);
    return { callId, signal, resources: this, principal: identity ? { subject: identity.subject, scopes: [`team:${identity.team}`] } : undefined };
  }
  access(context: Context<Store>, permission: "read" | "write" | "admin" = "read"): string {
    const team = this.team(context);
    const member = z.object({ role }).parse(this.db.prepare("SELECT role FROM desk_members WHERE team=? AND subject=?").get(team, requirePrincipal(context.principal).subject));
    if ((permission === "write" && member.role === "viewer") || (permission === "admin" && member.role !== "admin")) throw new ActionError("forbidden", "Role does not allow this operation");
    return team;
  }
  project(team: string, key: string) {
    const value = this.db.prepare("SELECT id,name,archived,version FROM desk_projects WHERE team=? AND id=?").get(team, key);
    if (!value) throw new ActionError("not_found", "Project not found");
    return projectSchema.parse(value);
  }
  item(team: string, key: string): Item {
    const value = this.db.prepare("SELECT id,project,title,assignee,status,version FROM desk_items WHERE team=? AND id=?").get(team, key);
    if (!value) throw new ActionError("not_found", "Item not found");
    return itemSchema.parse(value);
  }
  assignable(team: string, subject: string) {
    const member = this.db.prepare("SELECT role FROM desk_members WHERE team=? AND subject=?").get(team, subject);
    if (!member || role.parse(member.role) === "viewer") throw new ActionError("invalid_input", "Assignee must be a team editor or admin");
  }
  transaction<T>(run: () => T): T {
    this.db.exec("BEGIN IMMEDIATE");
    try { const result = run(); this.db.exec("COMMIT"); return result; }
    catch (error) { this.db.exec("ROLLBACK"); throw error; }
  }
  record(record: CallRecord) {
    this.db.prepare("INSERT INTO desk_calls(call_id,action,subject,outcome) VALUES(?,?,?,?)").run(record.callId, record.action, record.subject ?? null, record.outcome);
  }
  close() { this.db.close(); }
}

export function sameVersion(actual: number, expected: number) {
  if (actual !== expected) throw new ActionError("conflict", "Version changed; refresh before retrying");
}
export function active(archived: number) {
  if (archived) throw new ActionError("conflict", "Project is archived");
}
