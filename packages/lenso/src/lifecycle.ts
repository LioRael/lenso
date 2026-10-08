import { Effect, Exit, Scope } from "effect";
import { validatePlugins } from "./diagnostics";
import type { Contribution, Plugin, PluginContext } from "./plugin";

export interface RunningApp {
  get<T>(plugin: Plugin<T>): T;
  status(): readonly { id: string; state: "ready" | "stopped" }[];
  contributions(kind?: string): readonly Contribution[];
  stop(): Promise<void>;
}

/** Effect owns finalization only; plugin setup and business methods stay ordinary async. */
export async function startApp(app: { plugins: readonly Plugin<unknown>[] }): Promise<RunningApp> {
  const plugins = validatePlugins(app.plugins);
  const scope = await Effect.runPromise(Scope.make());
  const services = new Map<Plugin<unknown>, unknown>();
  const cleanupErrors: unknown[] = [];
  let running = true;
  let stopPromise: Promise<void> | undefined;

  function stop(): Promise<void> {
    if (!stopPromise) {
      running = false;
      stopPromise = Effect.runPromise(Scope.close(scope, Exit.void)).then(() => {
        services.clear();
        if (cleanupErrors.length) {
          throw new AggregateError(
            cleanupErrors,
            "Plugin cleanup failed. All registered finalizers were attempted.",
          );
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
        onCleanup(cleanup): void {
          if (!setupActive)
            throw new Error(`Plugin "${plugin.id}" setup context is no longer active.`);
          // An open sequential Scope registers synchronously and closes in LIFO order.
          Effect.runSync(
            Scope.addFinalizer(
              scope,
              Effect.promise(async () => {
                try {
                  await cleanup();
                } catch (error) {
                  cleanupErrors.push(error);
                }
              }),
            ),
          );
        },
      };
      try {
        services.set(plugin, await plugin.setup(context));
      } finally {
        setupActive = false;
      }
    }
  } catch (setupError) {
    try {
      await stop();
    } catch {
      throw new AggregateError(
        [setupError, ...cleanupErrors],
        "Plugin initialization failed and rollback reported cleanup errors.",
      );
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
