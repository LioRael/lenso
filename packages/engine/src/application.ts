import { access } from "node:fs/promises";
import { isAbsolute, relative, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { validatePlugins, type Contribution, type Logger, type Plugin } from "@lenso/core";
import { EngineError, diagnostic } from "./diagnostics";
import { validateOperations, type Operation, type OperationBinding } from "./operations";
import type { describePluginConfig } from "./configuration";

export type { OperationBinding } from "./operations";

/** A trusted application's root, with an optional config path relative to that root. */
export interface ApplicationTarget {
  readonly root: string;
  readonly config?: string;
}

export function resolveApplicationTarget(target: string | ApplicationTarget): ApplicationTarget {
  return typeof target === "string"
    ? { root: resolve(target) }
    : {
        root: resolve(target.root),
        ...(target.config === undefined ? {} : { config: target.config }),
      };
}

export function applicationConfigPath(target: string | ApplicationTarget): string {
  const { root, config } = resolveApplicationTarget(target);
  const path = resolve(root, config ?? "lenso.config.ts");
  const local = relative(root, path);
  if (
    !local ||
    local === ".." ||
    local.startsWith(`..${sep}`) ||
    isAbsolute(local) ||
    [".lenso", "dist"].includes(local.split(sep)[0]!)
  )
    throw new EngineError({
      code: "invalid-application-target",
      phase: "discovery",
      message: "Application config must be a file inside application root.",
      source: { file: path },
    });
  return path;
}

export interface AppDefinition<O extends Operation = Operation> {
  readonly instanceId?: string;
  readonly logger?: Logger;
  readonly plugins: readonly Plugin<unknown>[];
  readonly operations?: readonly O[];
  readonly mcpOperations?: readonly Operation[];
  readonly operationBinding?: OperationBinding<O>;
}
export interface PluginManifest {
  readonly id: string;
  readonly requires: readonly string[];
  readonly contributions: readonly Contribution[];
  readonly config?: ReturnType<typeof describePluginConfig>;
}
export interface Discovery {
  readonly root: string;
  readonly configPath: string;
  readonly app: AppDefinition;
  readonly ordered: readonly Plugin<unknown>[];
}
/** Imports trusted application config without loading Engine plugins or running setup. */
export async function readApplication(root: string, configPath: string): Promise<Discovery> {
  let loaded;
  try {
    await access(configPath);
    loaded = await import(pathToFileURL(configPath).href);
  } catch (cause) {
    throw new EngineError(
      {
        code: "config-load-failed",
        phase: "discovery",
        message: "Cannot load trusted application config.",
        source: { file: configPath },
      },
      { cause },
    );
  }
  const app: unknown = loaded.default;
  if (!app || typeof app !== "object" || !("plugins" in app) || !Array.isArray(app.plugins))
    throw new EngineError({
      code: "invalid-config",
      phase: "discovery",
      message: "Application config must default-export defineApp({ plugins: [...] })",
      source: { file: configPath },
    });
  const definition = app as AppDefinition;
  if (
    "operations" in definition ||
    "mcpOperations" in definition ||
    "operationBinding" in definition
  )
    throw new EngineError({
      code: "invalid-config",
      phase: "discovery",
      message:
        "Declare entry operations and bindings as named exports, not on the default application config.",
      source: { file: configPath },
    });
  try {
    const ordered = validatePlugins(definition.plugins);
    const operations = validateOperations(definition.plugins, loaded.operations ?? []);
    const mcpOperations =
      loaded.mcpOperations === undefined
        ? undefined
        : validateOperations(definition.plugins, loaded.mcpOperations);
    if (loaded.operationBinding !== undefined && typeof loaded.operationBinding !== "function")
      throw new EngineError({
        code: "invalid-config",
        phase: "discovery",
        message: "operationBinding must be a trusted entry function.",
      });
    return {
      root,
      configPath,
      app: {
        ...definition,
        operations,
        ...(mcpOperations === undefined ? {} : { mcpOperations }),
        ...(loaded.operationBinding === undefined
          ? {}
          : { operationBinding: loaded.operationBinding }),
      },
      ordered,
    };
  } catch (cause) {
    throw new EngineError(diagnostic(cause, { source: { file: configPath } }), { cause });
  }
}
