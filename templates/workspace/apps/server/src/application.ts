import { createGreetingPlugin } from "@app/greeting";
import { envSource } from "@lenso/core/config/env";

export const greeting = createGreetingPlugin([
  envSource({
    id: "greeting-env",
    read: (name) => process.env[name],
    bindings: { prefix: { name: "GREETING_PREFIX" } },
  }),
]);
