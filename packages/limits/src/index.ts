import { bindConfig, type ConfigSource } from "@lenso/core/config";
import type { Plugin, PluginContext } from "@lenso/core/plugin";
import { limitConfig } from "./config";
import type { LimitConfig, LimitStore } from "./contracts";
import { createLimits, type Limits } from "./service";

export type * from "./contracts";
export { LimitError } from "./contracts";
export { limitConfig, limitConfigSchema } from "./config";
export { createMemoryLimitStore } from "./memory";
export { createLimits } from "./service";
export type { Limits, LeaseExecution, LeaseRunOptions } from "./service";

/** The caller supplies exact dependencies; connect borrows a store and acquires no global context. */
export function createLimitsPlugin(options: {
  readonly id: string;
  readonly requires?: readonly Plugin<unknown>[];
  readonly config: LimitConfig | readonly ConfigSource[];
  readonly connect: (context: PluginContext) => LimitStore | Promise<LimitStore>;
}): Plugin<Limits> {
  return bindConfig(limitConfig, options.config, {
    id: options.id,
    requires: options.requires,
    async setup(context, config) {
      const store = await options.connect(context);
      const limits = createLimits({ store, config, logger: context.logger });
      const close = context.onCleanup(() => limits.close());
      return { ...limits, close };
    },
  });
}
