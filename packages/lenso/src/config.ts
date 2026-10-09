import type { StandardSchemaV1 } from "@standard-schema/spec";
import type {
  ConfigBinding,
  ConfigContract,
  ConfigDiagnostic,
  ConfigPath,
  ConfigProvenance,
  ConfigReadContext,
  ConfigSnapshot,
  ConfigSource,
  ConfigState,
} from "./config-types";
import type { ConfiguredPluginContext, Plugin, PluginSource } from "./plugin";

export type * from "./config-types";

const codes = new Set<ConfigDiagnostic["code"]>([
  "config-source-failed",
  "config-invalid-data",
  "config-invalid",
  "config-cancelled",
  "config-env-invalid",
  "config-file-missing",
  "config-file-invalid",
]);
const forbidden = new Set(["__proto__", "prototype", "constructor"]);

export class ConfigError extends Error {
  readonly diagnostics: readonly ConfigDiagnostic[];
  constructor(diagnostics: readonly ConfigDiagnostic[]) {
    super("Plugin configuration failed.");
    this.name = "ConfigError";
    this.diagnostics = Object.freeze(
      diagnostics.map((item) =>
        Object.freeze({
          code: codes.has(item.code) ? item.code : "config-source-failed",
          pluginId: safeText(item.pluginId),
          ...(item.path ? { path: safePath(item.path) } : {}),
          ...(item.sourceId !== undefined ? { sourceId: safeText(item.sourceId) } : {}),
          ...(item.source ? { source: safeLocation(item.source) } : {}),
        }),
      ),
    );
  }
}

export class ConfigSourceError extends Error {
  readonly code: ConfigDiagnostic["code"];
  readonly path?: ConfigPath;
  constructor(code: ConfigDiagnostic["code"], path?: ConfigPath) {
    super("Configuration source failed.");
    this.name = "ConfigSourceError";
    this.code = codes.has(code) ? code : "config-source-failed";
    if (path) this.path = safePath(path);
  }
}

function safeText(text: string): string {
  // Locations are declaration metadata, never a channel for URL credentials or queries.
  if (
    /[?#]|:\/\/[^/\s]*@|(?:bearer|basic)\s|(?:password|token|secret|api[_-]?key)(?:\s*[=:]|[/\\])/i.test(
      text,
    )
  )
    return "[redacted]";
  return text;
}

function safeLocation(source: PluginSource): PluginSource {
  return Object.freeze({
    file: safeText(source.file),
    ...(source.export !== undefined ? { export: safeText(source.export) } : {}),
    ...(Number.isSafeInteger(source.line) && source.line! > 0 ? { line: source.line } : {}),
    ...(Number.isSafeInteger(source.column) && source.column! > 0 ? { column: source.column } : {}),
  });
}

function pathKey(segment: unknown): unknown {
  return typeof segment === "object" && segment !== null
    ? Object.getOwnPropertyDescriptor(segment, "key")?.value
    : segment;
}

function safePath(path: readonly unknown[]): ConfigPath {
  const result: (string | number)[] = [];
  for (const segment of path) {
    const key = pathKey(segment);
    if (typeof key === "string") result.push(safeText(key));
    else if (typeof key === "number" && Number.isSafeInteger(key) && key >= 0) result.push(key);
    else return Object.freeze([]);
  }
  return Object.freeze(result);
}

function plain(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

class InvalidData {
  constructor(readonly path: ConfigPath) {}
}

function copyData(value: unknown, path: ConfigPath = [], ancestors = new Set<object>()): unknown {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value !== "object" || value === null || (!Array.isArray(value) && !plain(value)))
    throw new InvalidData(path);
  if (ancestors.has(value)) throw new InvalidData(path);
  ancestors.add(value);
  try {
    const array = Array.isArray(value);
    const result: Record<string, unknown> | unknown[] = array ? [] : {};
    for (const key of Reflect.ownKeys(value)) {
      if (array && key === "length") continue;
      if (typeof key !== "string") throw new InvalidData(path);
      const nextPath = [...path, array && /^\d+$/.test(key) ? Number(key) : key];
      if (forbidden.has(key)) throw new InvalidData(nextPath);
      const descriptor = Object.getOwnPropertyDescriptor(value, key)!;
      if (!("value" in descriptor) || !descriptor.enumerable) throw new InvalidData(nextPath);
      if (array && (!/^(0|[1-9]\d*)$/.test(key) || Number(key) >= (value as unknown[]).length))
        throw new InvalidData(nextPath);
      if (descriptor.value === undefined) {
        if (array) throw new InvalidData(nextPath);
        continue;
      }
      Object.defineProperty(result, key, {
        value: copyData(descriptor.value, nextPath, ancestors),
        enumerable: true,
        configurable: true,
        writable: true,
      });
    }
    if (array && Object.keys(result).length !== (value as unknown[]).length)
      throw new InvalidData(path);
    return result;
  } finally {
    ancestors.delete(value);
  }
}

function freezeData<T>(value: T): T {
  if (typeof value === "object" && value !== null) {
    for (const child of Object.values(value)) freezeData(child);
    Object.freeze(value);
  }
  return value;
}

function copyObject(value: unknown): Record<string, unknown> {
  if (!plain(value)) throw new InvalidData([]);
  return copyData(value) as Record<string, unknown>;
}

export function definePluginConfig<S extends StandardSchemaV1>(
  contract: ConfigContract<S>,
): ConfigContract<S> {
  return contract;
}

export function valuesSource(
  values: Readonly<Record<string, unknown>>,
  options: { id?: string; location?: PluginSource; sensitive?: readonly ConfigPath[] } = {},
): ConfigSource {
  return {
    descriptor: Object.freeze({
      id: safeText(options.id ?? "values"),
      kind: "values",
      ...(options.location ? { location: safeLocation(options.location) } : {}),
      ...(options.sensitive
        ? {
            fields: Object.freeze(
              options.sensitive.map((path) =>
                Object.freeze({
                  path: safePath(path),
                  sensitive: true,
                }),
              ),
            ),
          }
        : {}),
    }),
    async read() {
      try {
        return { values: copyObject(values) };
      } catch (error) {
        throw new ConfigSourceError(
          "config-invalid-data",
          error instanceof InvalidData ? error.path : [],
        );
      }
    },
  };
}

export function bindConfig<S extends StandardSchemaV1, T>(
  contract: ConfigContract<S>,
  input: StandardSchemaV1.InferInput<S> | readonly ConfigSource[],
  plugin: Omit<Plugin<T>, "setup" | "config"> & {
    setup(
      context: ConfiguredPluginContext,
      config: StandardSchemaV1.InferOutput<S>,
    ): T | Promise<T>;
  },
): Plugin<T> {
  const binding: ConfigBinding<S> = Object.freeze({
    contract,
    sources: Object.freeze(
      Array.isArray(input)
        ? ([...input] as ConfigSource[])
        : [valuesSource(input as Readonly<Record<string, unknown>>)],
    ),
  });
  return {
    ...plugin,
    config: binding,
    setup(context) {
      if (!context.config) throw new ConfigError([{ code: "config-invalid", pluginId: plugin.id }]);
      return plugin.setup(context as ConfiguredPluginContext, context.config(binding));
    },
  };
}

function cancelled(pluginId: string, context: ConfigReadContext): void {
  if (context.signal?.aborted) throw new ConfigError([{ code: "config-cancelled", pluginId }]);
}

export async function resolveConfig<S extends StandardSchemaV1>(
  pluginId: string,
  binding: ConfigBinding<S>,
  context: ConfigReadContext = {},
): Promise<ConfigSnapshot<StandardSchemaV1.InferOutput<S>>> {
  context = Object.freeze(context.signal ? { signal: context.signal } : {});
  const composed: Record<string, unknown> = {};
  const histories = new Map<string, string[]>();
  const locations = new Map<string, PluginSource | undefined>();
  const sensitive = new Set<string>();
  let allSensitive = false;
  const revisions: { sourceId: string; revision: unknown }[] = [];
  const sources: ConfigState["sources"][number][] = [];
  for (const field of binding.contract.fields ?? []) {
    if (field.sensitive && field.path.length === 0) allSensitive = true;
    if (field.sensitive && typeof field.path[0] === "string") sensitive.add(field.path[0]);
  }
  for (const source of binding.sources) {
    cancelled(pluginId, context);
    let sourceId: string | undefined;
    let location: PluginSource | undefined;
    try {
      const descriptor = source?.descriptor;
      if (!descriptor || typeof descriptor.id !== "string" || typeof source.read !== "function")
        throw new InvalidData([]);
      sourceId = safeText(descriptor.id);
      location = descriptor.location ? safeLocation(descriptor.location) : undefined;
      if (
        !descriptor.id.trim() ||
        typeof descriptor.kind !== "string" ||
        !descriptor.kind.trim() ||
        locations.has(sourceId)
      )
        throw new InvalidData([]);
      locations.set(sourceId, location);
      sources.push(Object.freeze({ id: sourceId, kind: safeText(descriptor.kind) }));
      for (const field of descriptor.fields ?? []) {
        if (field.sensitive && field.path.length === 0) allSensitive = true;
        if (field.sensitive && typeof field.path[0] === "string") sensitive.add(field.path[0]);
      }
      const result = await source.read(context);
      cancelled(pluginId, context);
      const valuesDescriptor = Object.getOwnPropertyDescriptor(result, "values");
      if (!valuesDescriptor || !("value" in valuesDescriptor)) throw new InvalidData([]);
      const values = copyObject(valuesDescriptor.value);
      for (const [key, value] of Object.entries(values)) {
        composed[key] = value;
        const history = histories.get(key) ?? [];
        history.push(sourceId);
        histories.set(key, history);
      }
      const revision = Object.getOwnPropertyDescriptor(result, "revision");
      if (revision && !("value" in revision)) throw new InvalidData([]);
      if (revision) revisions.push(Object.freeze({ sourceId, revision: revision.value }));
    } catch (error) {
      cancelled(pluginId, context);
      throw new ConfigError([
        {
          code:
            error instanceof ConfigSourceError
              ? error.code
              : error instanceof InvalidData
                ? "config-invalid-data"
                : "config-source-failed",
          pluginId,
          ...(sourceId !== undefined ? { sourceId } : {}),
          ...(location ? { source: location } : {}),
          ...(error instanceof ConfigSourceError || error instanceof InvalidData
            ? { path: error.path }
            : {}),
        },
      ]);
    }
  }
  cancelled(pluginId, context);
  let output: unknown;
  let validationError: ConfigError | undefined;
  try {
    const result = await binding.contract.schema["~standard"].validate(composed);
    cancelled(pluginId, context);
    if (result.issues) {
      validationError = new ConfigError(
        result.issues.map((issue) => {
          const path = issue.path ? safePath(issue.path) : undefined;
          const originalKey = pathKey(issue.path?.[0]);
          const history = typeof originalKey === "string" ? histories.get(originalKey) : undefined;
          const sourceId = history?.[history.length - 1];
          const source = sourceId === undefined ? undefined : locations.get(sourceId);
          return {
            code: "config-invalid",
            pluginId,
            ...(path ? { path } : {}),
            ...(sourceId === undefined ? {} : { sourceId }),
            ...(source ? { source } : {}),
          };
        }),
      );
      throw validationError;
    }
    output = freezeData(copyObject(result.value));
  } catch (error) {
    cancelled(pluginId, context);
    if (validationError !== undefined && error === validationError) throw error;
    throw new ConfigError([
      {
        code: error instanceof InvalidData ? "config-invalid-data" : "config-invalid",
        pluginId,
        ...(error instanceof InvalidData ? { path: error.path } : {}),
      },
    ]);
  }
  const provenance: ConfigProvenance[] = [...histories].map(([key, sourceIds]) =>
    Object.freeze({
      path: safePath([key]),
      sourceIds: Object.freeze(sourceIds),
      sensitive: allSensitive || sensitive.has(key),
    }),
  );
  return Object.freeze({
    value: output as StandardSchemaV1.InferOutput<S>,
    provenance: Object.freeze(provenance),
    revisions: Object.freeze(revisions),
    sources: Object.freeze(sources),
  });
}

export async function preflightConfigs(
  plugins: readonly Plugin<unknown>[],
  context: ConfigReadContext = {},
): Promise<ReadonlyMap<Plugin<unknown>, ConfigSnapshot>> {
  const snapshots = new Map<Plugin<unknown>, ConfigSnapshot>();
  const diagnostics: ConfigDiagnostic[] = [];
  cancelled(plugins[0]?.id ?? "", context);
  for (const plugin of plugins) {
    if (!plugin.config) continue;
    try {
      snapshots.set(plugin, await resolveConfig(plugin.id, plugin.config, context));
    } catch (error) {
      if (!(error instanceof ConfigError)) throw error;
      diagnostics.push(...error.diagnostics);
      if (context.signal?.aborted) break;
    }
  }
  if (diagnostics.length) throw new ConfigError(diagnostics);
  cancelled(plugins[0]?.id ?? "", context);
  return snapshots;
}
