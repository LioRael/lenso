import { createWebPlugin } from "@lenso/web";
import type { RunningApp } from "@lenso/core";
import { greeting } from "./greeting";
import { createRouter } from "./router";

export function createGreetingWeb(status: () => ReturnType<RunningApp["status"]>) {
  return createWebPlugin({
    requires: [greeting],
    router: (context) => createRouter(context.get(greeting), status),
  });
}
