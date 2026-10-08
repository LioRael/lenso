import { createBindingsPlugin, createWorkerHandler } from "@lenso/workers";
import { createWebPlugin, type WebContext } from "@lenso/web";
import { definePlugin } from "lenso/plugin";
import { os } from "@orpc/server";
import { z } from "zod";

export default createWorkerHandler<Env>((env) => {
  const bindings = createBindingsPlugin({ id: "bindings", bindings: env });
  const greeting = definePlugin({
    id: "greeting",
    requires: [bindings],
    setup(context) {
      const { GREETING_PREFIX } = context.get(bindings);
      let count = 0;
      return {
        async greet({ name }: { name: string }) {
          return { message: `${GREETING_PREFIX}, ${name.trim()}!`, count: ++count };
        },
      };
    },
  });
  const web = createWebPlugin({
    requires: [greeting],
    router(context) {
      const service = context.get(greeting);
      return {
        greet: os
          .$context<WebContext>()
          .input(z.object({ name: z.string().trim().min(2) }))
          .handler(({ input }) => service.greet(input)),
      };
    },
  });
  return { plugins: [bindings, greeting, web], web };
});
