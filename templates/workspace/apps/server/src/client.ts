import type { RouterClient } from "@orpc/server";
import type { createRouter } from "./router";

export type AppRouter = ReturnType<typeof createRouter>;
export type AppClient = RouterClient<AppRouter>;
