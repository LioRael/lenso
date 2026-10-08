import { access } from "node:fs/promises";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { startApp, validatePlugins, type Contribution, type Plugin } from "lenso";
import { CliError, diagnostic } from "./diagnostics";
import {
  describeOperation,
  redactOperationDescription,
  validateOperations,
  type Operation,
} from "./operations";
import { EngineSession, withEngine } from "./engine-host";
import { defaultEnginePlugins, pluginManifest } from "./engine-defaults";
import type { EngineMode, EngineSnapshot } from "./engine-authoring";

export interface AppDefinition {
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
async function readApplication(root: string, configPath: string): Promise<Discovery> {
  let loaded;
  try {
    await access(configPath);
    loaded = await import(pathToFileURL(configPath).href);
  } catch (cause) {
    throw new CliError(
      {
        code: "config-load-failed",
        phase: "discovery",
        message: "Cannot load trusted application config.",
        source: { file: configPath },
      },
      3,
      { cause },
    );
  }
  const app: unknown = loaded.default;
  if (!app || typeof app !== "object" || !("plugins" in app) || !Array.isArray(app.plugins))
    throw new CliError(
      {
        code: "invalid-config",
        phase: "discovery",
        message: "Application config must default-export defineApp({ plugins: [...] })",
        source: { file: configPath },
      },
      3,
    );
  const definition = app as AppDefinition;
  try {
    const ordered = validatePlugins(definition.plugins);
    const operations = validateOperations(
      definition.plugins,
      loaded.operations ?? definition.operations ?? [],
    );
    return { root, configPath, app: { ...definition, operations }, ordered };
  } catch (cause) {
    throw new CliError(diagnostic(cause, { source: { file: configPath } }), 3);
  }
}
export function createEngineSession(root: string, mode: EngineMode) {
  const session = new EngineSession(resolve(root), mode);
  let app: Promise<Discovery> | undefined;
  const readApp = (snapshot: EngineSnapshot) =>
    (app ??= readApplication(session.root, resolve(session.root, snapshot.convention.config)));
  return {
    session,
    async prepare() {
      await session.setup(defaultEnginePlugins(readApp));
      const snapshot = await session.discover();
      return readApp(snapshot);
    },
  };
}
/** Build-time discovery validates trusted extensions, then closes their resources. */
export async function discover(root = process.cwd()): Promise<Discovery> {
  const engine = createEngineSession(root, "check");
  return withEngine(engine.session, engine.prepare);
}
/** Static generation starts engine plugins, never runtime application plugin setup. */
export async function generate(root = process.cwd()): Promise<readonly PluginManifest[]> {
  const engine = createEngineSession(root, "generate");
  return withEngine(engine.session, async () => {
    const app = await engine.prepare();
    await engine.session.generate();
    return pluginManifest(app);
  });
}
export async function build(root = process.cwd(), entry?: string): Promise<string> {
  const engine = createEngineSession(root, "build");
  return withEngine(engine.session, async () => {
    await engine.prepare();
    await engine.session.generate();
    return engine.session.build(entry);
  });
}

export async function invoke(
  app: AppDefinition,
  pluginId: string,
  method: string,
  input: unknown,
): Promise<unknown> {
  const plugin = app.plugins.find((candidate) => candidate.id === pluginId);
  if (!plugin)
    throw new CliError(
      { code: "unknown-plugin", phase: "discovery", message: "Unknown plugin.", pluginId },
      3,
    );
  const operations = validateOperations(app.plugins, app.operations ?? []);
  const operation = operations.find(
    (candidate) => candidate.plugin === plugin && candidate.method === method,
  );
  const context = {
    pluginId,
    operation: `${pluginId}.${method}`,
    ...(operation?.source ? { source: operation.source } : {}),
  };
  if (!operation)
    throw new CliError(
      {
        code: "unknown-operation",
        phase: "discovery",
        message: "Service method is not explicitly exposed for CLI invocation.",
        ...context,
      },
      3,
    );
  let validated;
  try {
    validated = await operation.input["~standard"].validate(input);
  } catch (cause) {
    throw new CliError(
      { code: "invalid-input", phase: "input", message: "Input validation failed.", ...context },
      2,
      { cause },
    );
  }
  if (validated.issues)
    throw new CliError(
      {
        code: "invalid-input",
        phase: "input",
        message: "Input does not satisfy the shared service schema.",
        ...context,
        details: {
          paths: validated.issues.map((issue) =>
            (issue.path ?? []).map((segment) =>
              String(typeof segment === "object" ? segment.key : segment),
            ),
          ),
        },
      },
      2,
    );
  let running;
  try {
    running = await startApp(app);
  } catch (cause) {
    throw new CliError(diagnostic(cause, { ...context, phase: "setup" }), 1);
  }
  let result: unknown;
  let callFailed = false;
  let callError: unknown;
  try {
    const service = running.get(plugin);
    if (
      service === null ||
      typeof service !== "object" ||
      !Object.hasOwn(service, method) ||
      typeof Reflect.get(service, method) !== "function"
    ) {
      throw new CliError({
        code: "unavailable-operation",
        phase: "invoke",
        message: "Declared operation is not an own callable service method.",
        ...context,
      });
    }
    result = await Reflect.get(service, method).call(service, validated.value);
  } catch (cause) {
    callFailed = true;
    callError =
      cause instanceof CliError
        ? cause
        : new CliError(
            {
              code: "invocation-failed",
              phase: "invoke",
              message: "Service invocation failed.",
              ...context,
            },
            1,
            { cause },
          );
  }
  try {
    await running.stop();
  } catch (cleanupError) {
    if (callFailed)
      throw new CliError({
        code: "invocation-and-cleanup-failed",
        phase: "invoke",
        message: "Service invocation and cleanup failed.",
        ...context,
        causes: [
          diagnostic(callError, context),
          diagnostic(cleanupError, { ...context, phase: "cleanup" }),
        ],
      });
    throw new CliError(diagnostic(cleanupError, { ...context, phase: "cleanup" }));
  }
  if (callFailed) throw callError;
  return result;
}

export async function call(
  root: string,
  pluginId: string,
  method: string,
  input: unknown,
): Promise<unknown> {
  const directory = resolve(root);
  const { app } = await readApplication(directory, join(directory, "lenso.config.ts"));
  try {
    return await invoke(app, pluginId, method, input);
  } catch (cause) {
    throw new CliError(
      diagnostic(cause, { source: { file: join(resolve(root), "lenso.config.ts") } }),
      cause instanceof CliError ? cause.exitCode : 1,
    );
  }
}

/** Imports trusted config but never runs setup or discovers methods by reflection. */
export async function inspect(root = process.cwd(), pluginId?: string, method?: string) {
  const directory = resolve(root);
  const { app, ordered, configPath } = await readApplication(
    directory,
    join(directory, "lenso.config.ts"),
  );
  if (pluginId && !ordered.some((plugin) => plugin.id === pluginId))
    throw new CliError(
      {
        code: "unknown-plugin",
        phase: "discovery",
        message: "Unknown plugin.",
        pluginId,
        source: { file: configPath },
      },
      3,
    );
  const operations = (app.operations ?? []).filter(
    (operation) =>
      (!pluginId || operation.plugin.id === pluginId) && (!method || operation.method === method),
  );
  if (method && !operations.length)
    throw new CliError(
      {
        code: "unknown-operation",
        phase: "discovery",
        message: "Operation is not declared.",
        source: { file: configPath },
      },
      3,
    );
  return {
    configPath,
    plugins: ordered.map((plugin) => ({
      id: plugin.id,
      requires: (plugin.requires ?? []).map((dependency) => dependency.id),
      source: { file: configPath },
    })),
    operations: operations.map((operation) =>
      redactOperationDescription(describeOperation(operation, configPath)),
    ),
    limitations: [
      "Imports trusted config and executes module top-level code.",
      "Never runs plugin setup; runtime-only methods and authorization outcomes cannot be discovered.",
      "Each call starts and stops an isolated app; no automatic retry, cancellation or request drain.",
    ],
  };
}
