import { definePlugin, type Plugin, type PluginContext } from "lenso/plugin";

export function createAuthPlugin<T extends { close(): Promise<void> }>(options: {
  readonly id: string;
  readonly requires?: readonly Plugin<unknown>[];
  setup(context: PluginContext): T | Promise<T>;
}): Plugin<T> {
  return definePlugin({
    id: options.id,
    requires: options.requires,
    async setup(context) {
      const auth = await options.setup(context);
      context.onCleanup(() => auth.close());
      return auth;
    },
  });
}
