import { os, ORPCError } from "@orpc/server";
import type { WebContext } from "@lenso/web";
import type { RunningApp } from "@lenso/core";
import { greetingInput, GreetingInputError } from "./contracts";
import type { GreetingService } from "./greeting";

export function createRouter(
  service: GreetingService,
  status: () => ReturnType<RunningApp["status"]>,
) {
  const procedure = os.$context<WebContext>();
  return {
    greet: procedure.input(greetingInput).handler(async ({ input }) => {
      try {
        return await service.greet(input);
      } catch (error) {
        if (error instanceof GreetingInputError) {
          throw new ORPCError("BAD_REQUEST", { message: error.message });
        }
        throw error;
      }
    }),
    status: procedure.handler(() => status()),
  };
}

export type AppRouter = ReturnType<typeof createRouter>;
