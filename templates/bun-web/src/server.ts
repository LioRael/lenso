import { definePlugin, startApp } from "lenso";
import { createWebPlugin, type WebContext } from "@lenso/web";
import { os } from "@orpc/server";
import { z } from "zod";

export const greeting = definePlugin({
  id: "greeting",
  setup() {
    let count = 0;
    return {
      async greet({ name }: { name: string }) {
        return {
          message: `${process.env.GREETING_PREFIX ?? "Hello"}, ${name.trim()}!`,
          count: ++count,
        };
      },
    };
  },
});

if (import.meta.main) {
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
  const listener = definePlugin({
    id: "http",
    requires: [web],
    setup(context) {
      const service = context.get(web);
      const server = Bun.serve({
        hostname: "127.0.0.1",
        port: Number(process.env.LENSO_PORT ?? 3000),
        fetch: (request) => service.fetch(request),
      });
      context.onCleanup(() => server.stop(true));
      return server.url;
    },
  });
  const app = await startApp({ plugins: [greeting, web, listener] });
  console.log(`Ready at ${app.get(listener)}`);
  const stop = () => {
    void app.stop().catch(console.error);
  };
  process.once("SIGINT", stop);
  process.once("SIGTERM", stop);
}
