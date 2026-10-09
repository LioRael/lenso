import type { Plugin, PluginSource } from "./plugin";

export interface Diagnostic {
  readonly code:
    | "duplicate-id"
    | "missing-dependency"
    | "cyclic-dependency"
    | "invalid-id"
    | "invalid-source"
    | "invalid-plugin";
  readonly pluginId: string;
  readonly message: string;
  readonly source?: PluginSource;
  readonly details?: unknown;
  readonly dependencyId?: string;
}

export class DiagnosticError extends Error {
  readonly diagnostics: readonly Diagnostic[];

  constructor(diagnostics: readonly Diagnostic[]) {
    super(diagnostics.map((diagnostic) => diagnostic.message).join("\n"));
    this.name = "DiagnosticError";
    this.diagnostics = diagnostics;
  }
}

function pluginDiagnostics(plugins: readonly Plugin<unknown>[], requireDependencies: boolean) {
  const diagnostics: Diagnostic[] = [];
  const validShape = (value: unknown): value is Plugin<unknown> =>
    value !== null &&
    typeof value === "object" &&
    typeof Reflect.get(value, "id") === "string" &&
    typeof Reflect.get(value, "setup") === "function";
  for (const [index, plugin] of plugins.entries()) {
    if (
      !validShape(plugin) ||
      (plugin.requires !== undefined &&
        (!Array.isArray(plugin.requires) || !Array.from(plugin.requires).every(validShape)))
    ) {
      diagnostics.push({
        code: "invalid-plugin",
        pluginId: "[invalid]",
        message: "Plugin declarations require a string ID, setup function and plugin dependencies.",
        details: { path: ["plugins", index] },
      });
    }
  }
  if (diagnostics.length) throw new DiagnosticError(diagnostics);
  const instances = new Set(plugins);
  const firstById = new Map<string, Plugin<unknown>>();
  const sources = new Map<Plugin<unknown>, PluginSource>();
  for (const plugin of plugins) {
    const source = plugin.source;
    if (
      source !== undefined &&
      (!source ||
        typeof source !== "object" ||
        typeof source.file !== "string" ||
        !source.file.trim() ||
        (source.export !== undefined && typeof source.export !== "string") ||
        (source.line !== undefined && (!Number.isInteger(source.line) || source.line < 0)) ||
        (source.column !== undefined && (!Number.isInteger(source.column) || source.column < 0)))
    ) {
      diagnostics.push({
        code: "invalid-source",
        pluginId: plugin.id,
        message: `Plugin "${plugin.id}" has invalid source metadata.`,
      });
    } else if (source) {
      sources.set(plugin, source);
    }
    if (!plugin.id.trim()) {
      diagnostics.push({
        code: "invalid-id",
        pluginId: plugin.id,
        message: "Plugin IDs must not be empty.",
        ...(sources.has(plugin) ? { source: sources.get(plugin)! } : {}),
      });
    }
    const first = firstById.get(plugin.id);
    if (first) {
      diagnostics.push({
        code: "duplicate-id",
        pluginId: plugin.id,
        message: `Duplicate plugin ID "${plugin.id}".`,
        ...(sources.has(plugin) ? { source: sources.get(plugin)! } : {}),
        details: {
          declaringSources: [sources.get(first), sources.get(plugin)].filter(Boolean),
        },
      });
    }
    if (!firstById.has(plugin.id)) firstById.set(plugin.id, plugin);
    for (const dependency of requireDependencies ? (plugin.requires ?? []) : []) {
      if (!instances.has(dependency)) {
        diagnostics.push({
          code: "missing-dependency",
          pluginId: plugin.id,
          message: `Plugin "${plugin.id}" requires missing instance "${dependency.id}". Include the same plugin object in the app.`,
          dependencyId: dependency.id,
          ...(sources.has(plugin) ? { source: sources.get(plugin)! } : {}),
        });
      }
    }
  }
  return { diagnostics, sources };
}

/** Validate declaration metadata for a selection, without requiring its dependency graph. */
export function validatePluginSelection(plugins: readonly Plugin<unknown>[]): void {
  const { diagnostics } = pluginDiagnostics(plugins, false);
  if (diagnostics.length) throw new DiagnosticError(diagnostics);
}

/** Validate exact plugin instances and return a stable dependency-first order. */
export function validatePlugins(plugins: readonly Plugin<unknown>[]): readonly Plugin<unknown>[] {
  const { diagnostics, sources } = pluginDiagnostics(plugins, true);
  const instances = new Set(plugins);
  const order: Plugin<unknown>[] = [];
  const visited = new Set<Plugin<unknown>>();
  const stack: Plugin<unknown>[] = [];
  const visiting = new Set<Plugin<unknown>>();
  function visit(plugin: Plugin<unknown>): void {
    if (visited.has(plugin)) return;
    if (visiting.has(plugin)) {
      const cycle = [...stack.slice(stack.indexOf(plugin)), plugin].map((item) => item.id);
      diagnostics.push({
        code: "cyclic-dependency",
        pluginId: plugin.id,
        message: `Cyclic plugin dependency: ${cycle.join(" -> ")}.`,
        ...(sources.has(plugin) ? { source: sources.get(plugin)! } : {}),
      });
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
