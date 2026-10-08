import type { D1Database } from "@cloudflare/workers-types";
import { definePlugin } from "@lenso/core";
import { createD1Plugin } from "@lenso/db/d1";
import { createWorkerHandler } from "@lenso/workers";
import { createLimitsPlugin, type ConsumeInput } from "../src/index";
import { createD1LimitStore, d1LimitSchema } from "../src/d1";

/** Local test fixture only: not a public scope-taking application endpoint. */
export default createWorkerHandler<{ DB: D1Database }>((bindings) => {
  const db = createD1Plugin({ id: "limit-db", binding: bindings.DB, schema: d1LimitSchema });
  const limits = createLimitsPlugin({
    id: "limits",
    requires: [db],
    config: { failurePolicy: "throw" },
    connect: (context) => createD1LimitStore(context.get(db)),
  });
  const web = definePlugin({
    id: "fixture",
    requires: [limits],
    setup(context) {
      const service = context.get(limits);
      return {
        async fetch(request: Request) {
          const input = (await request.json()) as ConsumeInput;
          return Response.json(await service.consumeRate(input));
        },
      };
    },
  });
  return { plugins: [db, limits, web], web };
});
