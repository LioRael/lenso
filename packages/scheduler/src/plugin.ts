import { definePlugin, type Plugin, type PluginContext } from "@lenso/core/plugin";
import type { Actor } from "@lenso/auth";
import type { TaskQueue } from "@lenso/tasks";
import { createScheduler, type Scheduler, type SchedulerOptions } from "./index";

/** Providers are borrowed from exact installed instances; no worker/driver starts here. */
export function createSchedulerPlugin<D, A extends Actor = Actor>(options: {
  readonly id: string;
  readonly database: Plugin<D>;
  readonly queue: Plugin<TaskQueue>;
  readonly connect: (
    database: D,
    context: PluginContext,
  ) => SchedulerOptions["store"] | Promise<SchedulerOptions["store"]>;
  readonly options: Omit<SchedulerOptions<A>, "store" | "queue" | "logger">;
}): Plugin<Scheduler<A>> {
  return definePlugin({
    id: options.id,
    requires: [options.database, options.queue],
    async setup(context) {
      const store = await options.connect(context.get(options.database), context);
      return createScheduler({
        ...options.options,
        store,
        queue: context.get(options.queue),
        logger: context.logger,
      });
    },
  });
}
