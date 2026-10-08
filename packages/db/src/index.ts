import { definePlugin, type Plugin, type PluginContext } from "@lenso/core/plugin";

/** Keeps the native Drizzle database type. No common ORM or implicit migrations. */
export function createDrizzlePlugin<T>(options: {
  id: string;
  requires?: readonly Plugin<unknown>[];
  connect(context: PluginContext): T | Promise<T>;
}): Plugin<T> {
  return definePlugin({ id: options.id, requires: options.requires, setup: options.connect });
}
