import { validatePlugins } from "./diagnostics";
import type { Contribution, Plugin, PluginContext, PluginSource } from "./plugin";

export interface RunningApp {
  get<T>(plugin: Plugin<T>): T;
  status(): readonly { id: string; state: "ready" | "stopped" }[];
  contributions(kind?: string): readonly Contribution[];
  stop(): Promise<void>;
}

/** Serial setup and explicit LIFO cleanup; business methods stay ordinary async. */
export async function startApp(app: { plugins: readonly Plugin<unknown>[] }): Promise<RunningApp> {
  const plugins = validatePlugins(app.plugins);
  const finalizers: Array<{
    pluginId: string;
    source?: PluginSource;
    cleanup: () => void | Promise<void>;
  }> = [];
  const services = new Map<Plugin<unknown>, unknown>();
  const cleanupErrors: unknown[] = [];
  let running = true;
  let stopPromise: Promise<void> | undefined;

  function stop(): Promise<void> {
    if (!stopPromise) {
      running = false;
      // Cache before running callbacks, including synchronous reentrant stop calls.
      stopPromise = Promise.resolve().then(async () => {
        while (finalizers.length) {
          const { pluginId, source, cleanup } = finalizers.pop()!;
          try {
            await cleanup();
          } catch (error) {
            recordFailure(error, { phase: "cleanup", pluginId, ...(source ? { source } : {}) });
            cleanupErrors.push(error);
          }
        }
        services.clear();
        if (cleanupErrors.length) {
          const error = new AggregateError(
            cleanupErrors,
            "Plugin cleanup failed. All registered finalizers were attempted.",
          );
          recordFailure(error, { phase: "cleanup" });
          throw error;
        }
      });
    }
    return stopPromise;
  }

  try {
    for (const plugin of plugins) {
      let setupActive = true;
      const declared = new Set(plugin.requires ?? []);
      const context: PluginContext = {
        get<T>(dependency: Plugin<T>): T {
          if (!running) throw new Error("The app is stopped.");
          if (!declared.has(dependency)) {
            throw new Error(
              `Plugin "${plugin.id}" requested undeclared dependency "${dependency.id}".`,
            );
          }
          if (!services.has(dependency)) {
            throw new Error(
              `Dependency "${dependency.id}" for plugin "${plugin.id}" is not initialized.`,
            );
          }
          return services.get(dependency) as T;
        },
        onCleanup(cleanup) {
          if (!setupActive)
            throw new Error(`Plugin "${plugin.id}" setup context is no longer active.`);
          let completion: Promise<void> | undefined;
          const dispose = () =>
            (completion ??= Promise.resolve().then(async () => {
              try {
                await cleanup();
              } catch (error) {
                recordFailure(error, {
                  phase: "cleanup",
                  pluginId: plugin.id,
                  ...(plugin.source ? { source: plugin.source } : {}),
                });
                throw error;
              }
            }));
          finalizers.push({
            pluginId: plugin.id,
            ...(plugin.source ? { source: plugin.source } : {}),
            cleanup: dispose,
          });
          return dispose;
        },
      };
      try {
        services.set(plugin, await plugin.setup(context));
      } catch (error) {
        recordFailure(error, {
          phase: "setup",
          pluginId: plugin.id,
          ...(plugin.source ? { source: plugin.source } : {}),
        });
        throw error;
      } finally {
        setupActive = false;
      }
    }
  } catch (setupError) {
    try {
      await stop();
    } catch {
      const error = new AggregateError(
        [setupError, ...cleanupErrors],
        "Plugin initialization failed and rollback reported cleanup errors.",
      );
      recordFailure(error, { ...lifecycleFailure(setupError), phase: "setup" });
      throw error;
    }
    throw setupError;
  }

  return {
    get<T>(plugin: Plugin<T>): T {
      if (!running) throw new Error("The app is stopped.");
      if (!services.has(plugin))
        throw new Error(`Plugin instance "${plugin.id}" is not part of this app.`);
      return services.get(plugin) as T;
    },
    status: () =>
      plugins.map((plugin) => ({ id: plugin.id, state: running ? "ready" : "stopped" })),
    contributions: (kind) =>
      plugins.flatMap((plugin) =>
        (plugin.contributions ?? []).filter((item) => kind === undefined || item.kind === kind),
      ),
    stop,
  };
}

export interface LifecycleFailure {
  readonly phase: "setup" | "cleanup";
  readonly pluginId?: string;
  readonly source?: PluginSource;
}
const failures = new WeakMap<object, LifecycleFailure>();
function recordFailure(error: unknown, failure: LifecycleFailure): void {
  if ((typeof error === "object" && error !== null) || typeof error === "function") {
    failures.set(error, failure);
  }
}
/** Read diagnostic attribution without wrapping or mutating the original error. */
export function lifecycleFailure(error: unknown): LifecycleFailure | undefined {
  return (typeof error === "object" && error !== null) || typeof error === "function"
    ? failures.get(error)
    : undefined;
}
