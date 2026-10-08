import { access } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { validatePlugins, type Contribution, type Logger, type Plugin } from "lenso";
import { EngineError, diagnostic } from "./diagnostics";
import { validateOperations, type Operation } from "./operations";

export interface AppDefinition {
  readonly instanceId?: string;
  readonly logger?: Logger;
  readonly plugins: readonly Plugin<unknown>[];
  readonly operations?: readonly Operation[];
}
export interface PluginManifest {
  readonly id: string;
  readonly requires: readonly string[];
  readonly contributions: readonly Contribution[];
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
  if ("operations" in definition)
    throw new EngineError({
      code: "invalid-config",
      phase: "discovery",
      message: "Declare CLI operations as a named export, not on the default application config.",
      source: { file: configPath },
    });
  try {
    const ordered = validatePlugins(definition.plugins);
    const operations = validateOperations(definition.plugins, loaded.operations ?? []);
    return { root, configPath, app: { ...definition, operations }, ordered };
  } catch (cause) {
    throw new EngineError(diagnostic(cause, { source: { file: configPath } }), { cause });
  }
}
