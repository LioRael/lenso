import type { Plugin } from './plugin';

export interface Diagnostic {
  readonly code: 'duplicate-id' | 'missing-dependency' | 'cyclic-dependency' | 'invalid-id';
  readonly pluginId: string;
  readonly message: string;
}

export class DiagnosticError extends Error {
  readonly diagnostics: readonly Diagnostic[];

  constructor(diagnostics: readonly Diagnostic[]) {
    super(diagnostics.map((diagnostic) => diagnostic.message).join('\n'));
    this.name = 'DiagnosticError';
    this.diagnostics = diagnostics;
  }
}

/** Validate exact plugin instances and return a stable dependency-first order. */
export function validatePlugins(plugins: readonly Plugin<unknown>[]): readonly Plugin<unknown>[] {
  const diagnostics: Diagnostic[] = [];
  const instances = new Set(plugins);
  const ids = new Set<string>();
  for (const plugin of plugins) {
    if (!plugin.id.trim()) {
      diagnostics.push({ code: 'invalid-id', pluginId: plugin.id, message: 'Plugin IDs must not be empty.' });
    }
    if (ids.has(plugin.id)) {
      diagnostics.push({ code: 'duplicate-id', pluginId: plugin.id, message: `Duplicate plugin ID "${plugin.id}".` });
    }
    ids.add(plugin.id);
    for (const dependency of plugin.requires ?? []) {
      if (!instances.has(dependency)) {
        diagnostics.push({
          code: 'missing-dependency',
          pluginId: plugin.id,
          message: `Plugin "${plugin.id}" requires missing instance "${dependency.id}". Include the same plugin object in the app.`,
        });
      }
    }
  }

  const order: Plugin<unknown>[] = [];
  const visited = new Set<Plugin<unknown>>();
  const stack: Plugin<unknown>[] = [];
  const visiting = new Set<Plugin<unknown>>();
  function visit(plugin: Plugin<unknown>): void {
    if (visited.has(plugin)) return;
    if (visiting.has(plugin)) {
      const cycle = [...stack.slice(stack.indexOf(plugin)), plugin].map((item) => item.id);
      diagnostics.push({ code: 'cyclic-dependency', pluginId: plugin.id, message: `Cyclic plugin dependency: ${cycle.join(' -> ')}.` });
      return;
    }
    visiting.add(plugin);
    stack.push(plugin);
    for (const dependency of plugin.requires ?? []) {
      if (instances.has(dependency)) visit(dependency);
    }
    stack.pop();
    visiting.delete(plugin);
    visited.add(plugin);
    order.push(plugin);
  }
  for (const plugin of plugins) visit(plugin);
  if (diagnostics.length) throw new DiagnosticError(diagnostics);
  return order;
}
