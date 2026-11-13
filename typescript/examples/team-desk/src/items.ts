import { z } from "zod";
import { ActionError, DurableEvent, Module, defineAction, emptyInput, requirePrincipal } from "@lenso/app";
import type { Context } from "@lenso/app";
import { Store, active, sameVersion } from "./store.js";
import { id, itemSchema, itemInput, assignInput, transitionInput } from "./contracts.js";
import type { Change } from "./contracts.js";

export function itemsModule() {
  const changed = new DurableEvent<Change>("items.changed");
  const action = defineAction<Store>();
  function publish(context: Context<Store>, key: string, kind: Change["kind"]) {
    const store = context.resources, team = store.team(context), item = store.item(team, key);
    store.queue.publish(changed, `${team}/${key}/${item.version}`, { team, recipient: item.assignee, item: key, actor: requirePrincipal(context.principal).subject, kind, version: item.version });
    return item;
  }
  const list = action({
    id: "items.list", input: emptyInput, output: z.array(itemSchema),
    authorize(context) { context.resources.access(context); },
    run(context) { return z.array(itemSchema).parse(context.resources.db.prepare("SELECT id,project,title,assignee,status,version FROM desk_items WHERE team=? ORDER BY id").all(context.resources.team(context))); },
  });
  const detail = action({
    id: "items.detail", input: z.object({ id }), output: itemSchema,
    authorize(context) { context.resources.access(context); },
    run(context, input) { return context.resources.item(context.resources.team(context), input.id); },
  });
  const create = action({
    id: "items.create", input: itemInput, output: itemSchema,
    authorize(context) { context.resources.access(context, "write"); },
    run(context, input) {
      const store = context.resources, team = store.team(context);
      return store.transaction(() => {
        active(store.project(team, input.project).archived);
        store.assignable(team, input.assignee);
        const result = store.db.prepare("INSERT INTO desk_items(team,id,project,title,assignee,status) VALUES(?,?,?,?,?,'open') ON CONFLICT DO NOTHING").run(team, input.id, input.project, input.title, input.assignee);
        if (!result.changes) throw new ActionError("conflict", "Item already exists");
        return publish(context, input.id, "created");
      });
    },
  });
  const assign = action({
    id: "items.assign", input: assignInput, output: itemSchema,
    authorize(context) { context.resources.access(context, "write"); },
    run(context, input) {
      const store = context.resources, team = store.team(context);
      return store.transaction(() => {
        const item = store.item(team, input.id);
        sameVersion(item.version, input.version); active(store.project(team, item.project).archived);
        store.assignable(team, input.assignee);
        store.db.prepare("UPDATE desk_items SET assignee=?,version=version+1 WHERE team=? AND id=?").run(input.assignee, team, input.id);
        return publish(context, input.id, "assigned");
      });
    },
  });
  const transition = action({
    id: "items.transition", input: transitionInput, output: itemSchema,
    authorize(context) { context.resources.access(context, "write"); },
    run(context, input) {
      const store = context.resources, team = store.team(context);
      return store.transaction(() => {
        const item = store.item(team, input.id);
        sameVersion(item.version, input.version); active(store.project(team, item.project).archived);
        if (!((item.status === "open" && input.status === "doing") || (item.status === "doing" && input.status === "done"))) throw new ActionError("conflict", "Invalid status transition");
        store.db.prepare("UPDATE desk_items SET status=?,version=version+1 WHERE team=? AND id=?").run(input.status, team, input.id);
        return publish(context, input.id, "status");
      });
    },
  });
  return {
    module: new Module("items", { actions: { list, detail, create, assign, transition }, events: { changed }, resources: ["sqlite"] }),
    pages: [{ id: "items", title: "Work items", list, detail, actions: [create, assign, transition] }],
    actions: { list, detail, create, assign, transition }, changed,
  };
}
