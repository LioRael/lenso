import { z } from "zod";
import { Module, defineAction, defineTask, emptyInput, jsonCodec, requirePrincipal } from "@lenso/app";
import { Store } from "./store.js";

import { changeSchema } from "./contracts.js";

export function notificationsModule() {
  const deliver = defineTask<Store>()({
    id: "notifications.deliver", input: jsonCodec(changeSchema), output: jsonCodec(emptyInput),
    run(context, input) {
      context.resources.db.prepare("INSERT INTO desk_notifications(job_id,team,recipient,item,actor,kind,version) VALUES(?,?,?,?,?,?,?) ON CONFLICT(job_id) DO NOTHING")
        .run(context.jobId, input.team, input.recipient, input.item, input.actor, input.kind, input.version);
    },
  });
  const action = defineAction<Store>();
  const list = action({
    id: "notifications.list", input: emptyInput, output: z.array(changeSchema),
    authorize(context) { context.resources.access(context); },
    run(context) { return z.array(changeSchema).parse(context.resources.db.prepare("SELECT team,recipient,item,actor,kind,version FROM desk_notifications WHERE team=? AND recipient=? ORDER BY item,version").all(context.resources.team(context), requirePrincipal(context.principal).subject)); },
  });
  const drain = action({
    id: "notifications.drain", input: emptyInput, output: z.object({ completed: z.number().int() }),
    authorize(context) { context.resources.access(context, "admin"); },
    async run(context) {
      let completed = 0;
      while (completed < 100 && await context.resources.queue.runOne(deliver, context.resources)) completed++;
      return { completed };
    },
  });
  return {
    module: new Module("notifications", { actions: { list, drain }, tasks: { deliver }, resources: ["sqlite"] }),
    pages: [{ id: "notifications", title: "My notifications", list }],
    actions: { list, drain }, deliver,
  };
}
