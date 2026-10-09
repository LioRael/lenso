import { greetingInput, type createGreetingService } from "@app/greeting";
import type { WebContext } from "@lenso/web";
import { os } from "@orpc/server";

export function createRouter(service: ReturnType<typeof createGreetingService>) {
  return {
    greet: os
      .$context<WebContext>()
      .input(greetingInput)
      .handler(({ input }) => service.greet(input)),
  };
}
