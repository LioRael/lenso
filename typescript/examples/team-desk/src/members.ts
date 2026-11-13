import { z } from "zod";
import { ActionError, Module, defineAction, emptyInput, requirePrincipal } from "@lenso/app";
import { Store } from "./store.js";
import { memberSchema, sessionSchema, setRoleInput } from "./contracts.js";

export function membersModule() {
  const member = memberSchema;
  const action = defineAction<Store>();
  const session = action({
    id: "members.session", input: emptyInput, output: sessionSchema,
    authorize(context) { context.resources.access(context); },
    run(context) {
      const subject = requirePrincipal(context.principal).subject;
      const team = context.resources.team(context);
      return sessionSchema.parse({ ...context.resources.db.prepare("SELECT role FROM desk_members WHERE team=? AND subject=?").get(team, subject), team, subject });
    },
  });
  const list = action({
    id: "members.list", input: emptyInput, output: z.array(member),
    authorize(context) { context.resources.access(context); },
    run(context) { return z.array(member).parse(context.resources.db.prepare("SELECT subject,role FROM desk_members WHERE team=? ORDER BY subject").all(context.resources.team(context))); },
  });
  const setRole = action({
    id: "members.setRole", input: setRoleInput, output: member,
    authorize(context) { context.resources.access(context, "admin"); },
    run(context, input) {
      if (input.subject === requirePrincipal(context.principal).subject) throw new ActionError("conflict", "Cannot change your own role");
      const changed = context.resources.db.prepare("UPDATE desk_members SET role=? WHERE team=? AND subject=?").run(input.role, context.resources.team(context), input.subject);
      if (!changed.changes) throw new ActionError("not_found", "Member not found");
      return input;
    },
  });
  return {
    module: new Module("members", { actions: { list, setRole, session }, resources: ["sqlite"] }),
    pages: [{ id: "members", title: "Members", list, actions: [setRole] }],
    actions: { list, setRole, session },
  };
}
