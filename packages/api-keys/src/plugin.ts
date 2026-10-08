import { definePlugin, type Plugin, type PluginContext } from "@lenso/core/plugin";
import { createApiKeys, type ApiKeyOptions, type ApiKeys } from "./index";

export function createApiKeyPlugin<C, R>(options: {
  readonly id: string;
  readonly requires?: readonly Plugin<unknown>[];
  readonly config?: Plugin<unknown>["config"];
  readonly setup: (context: PluginContext) => ApiKeyOptions<C, R> | Promise<ApiKeyOptions<C, R>>;
}): Plugin<ApiKeys<C, R>> {
  return definePlugin({
    id: options.id,
    requires: options.requires,
    config: options.config,
    async setup(context) {
      const service = createApiKeys(await options.setup(context));
      context.onCleanup(() => service.close());
      return service;
    },
  });
}
