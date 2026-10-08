import { definePlugin, startApp } from "@lenso/core";
import { createWebPlugin, type WebContext } from "@lenso/web";
import { createBunListenerPlugin } from "@lenso/web/bun";
import { os } from "@orpc/server";
import { z } from "zod";
import { reportDevReady } from "@lenso/engine/dev-ready";

export const greetingInput = z.object({ name: z.string().trim().min(2) });

export const greeting = definePlugin({
  id: "greeting",
  setup() {
    let count = 0;
    return {
      async greet(input: z.output<typeof greetingInput>) {
        const { name } = greetingInput.parse(input);
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
          .input(greetingInput)
          .handler(({ input }) => service.greet(input)),
      };
    },
  });
  const listener = createBunListenerPlugin({
    id: "http",
    web,
    hostname: "127.0.0.1",
    port: Number(process.env.LENSO_PORT ?? 3000),
    ingress: () => undefined,
  });
  const app = await startApp({ plugins: [greeting, web, listener] });
  console.log(`Ready at ${app.get(listener).url}`);
  reportDevReady({ urls: [app.get(listener).url], capabilities: ["greeting.greet via Web/oRPC"] });
  const stop = () => {
    void app.stop().catch(console.error);
  };
  process.once("SIGINT", stop);
  process.once("SIGTERM", stop);
}
