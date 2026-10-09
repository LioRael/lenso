import type { RouterClient } from "@orpc/server";
import { os } from "@orpc/server";
import type { WebContext } from "@lenso/web";
import { greetingInput, type createGreetingService } from "./greeting";

export function createRouter(service: ReturnType<typeof createGreetingService>) {
  return {
    greet: os
      .$context<WebContext>()
      .input(greetingInput)
      .handler(({ input }) => service.greet(input)),
  };
}

export type AppRouter = ReturnType<typeof createRouter>;
export type AppClient = RouterClient<AppRouter>;
