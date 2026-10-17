import { validatePlugins, validatePluginSelection } from "./diagnostics";
import type { Plugin } from "./plugin";

/** Explicitly include the exact transitive dependencies of selected roots, once each. */
export function collectPlugins(roots: readonly Plugin<unknown>[]): readonly Plugin<unknown>[] {
  const plugins: Plugin<unknown>[] = [];
  const seen = new Set<Plugin<unknown>>();
  function collect(plugin: Plugin<unknown>): void {
    if (seen.has(plugin)) return;
    // Validate before traversing: malformed declarations should retain structured diagnostics.
    validatePluginSelection([plugin]);
    seen.add(plugin);
    plugins.push(plugin);
    for (const dependency of plugin.requires ?? []) collect(dependency);
  }
  for (const root of roots) collect(root);
  return validatePlugins(plugins);
}
