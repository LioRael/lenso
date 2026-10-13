import { startApp, type RunningApp } from "@lenso/core";
import { definePlugin, type Plugin, type PluginContext } from "@lenso/core/plugin";
import { RuntimeHost } from "@lenso/runtime-sdk";

export type RuntimeConnection = Parameters<typeof RuntimeHost.connect>[0];
export type RuntimeHandlers = NonNullable<RuntimeConnection["handlers"]>;
const activateRuntime: unique symbol = Symbol("activateRuntime");
type RuntimePlugin = Plugin<unknown> & {
  readonly [activateRuntime]?: (running: RunningApp) => Promise<void>;
};

export interface RuntimeService<Clients = undefined> {
  readonly host: RuntimeHost;
  readonly clients: Clients;
  readonly disposer?: () => Promise<void>;
}

export interface RuntimePluginOptions<Clients = undefined> {
  readonly id: string;
  readonly requires?: readonly Plugin<unknown>[];
  readonly connection: Omit<RuntimeConnection, "handlers" | "instance" | "ready"> & {
    readonly instance?: string;
  };
  /** Only these explicitly supplied handlers are exported across the runtime boundary. */
  readonly handlers?: (context: PluginContext) => RuntimeHandlers;
  /** Bind native services only after every application plugin has completed setup. */
  readonly exports?: (running: RunningApp, clients: Clients) => RuntimeHandlers;
  readonly clients: (host: RuntimeHost) => Clients;
  readonly drainTimeoutMs?: number;
}

export function createRuntimePlugin<Clients = undefined>(
  options: RuntimePluginOptions<Clients>,
): Plugin<RuntimeService<Clients>> {
  if (options.handlers && options.exports) {
    throw new Error("Runtime plugin must select either handlers or exports, not both.");
  }
  const activations = new WeakMap<RuntimeService<Clients>, Promise<void>>();
  const plugin = definePlugin({
    id: options.id,
    requires: options.requires,
    async setup(context) {
      const host = await RuntimeHost.connect({
        ...options.connection,
        instance: options.connection.instance ?? context.instanceId,
        handlers: options.handlers?.(context),
        ready: false,
      });
      const disposer = context.onCleanup(async () => {
        const errors: unknown[] = [];
        try {
          await host.drain(options.drainTimeoutMs ?? 5000);
        } catch (error) {
          errors.push(error);
        }
        try {
          await host.close();
        } catch (error) {
          errors.push(error);
        }
        if (errors.length === 1) throw errors[0];
        if (errors.length > 1) throw new AggregateError(errors, "Runtime host cleanup failed.");
      });
      return { host, clients: options.clients(host), disposer };
    },
  });
  Object.defineProperty(plugin, activateRuntime, {
    value(running: RunningApp): Promise<void> {
      const service = running.get(plugin);
      let activation = activations.get(service);
      if (!activation) {
        activation = Promise.resolve().then(() =>
          service.host.activate(options.exports?.(running, service.clients)),
        );
        activations.set(service, activation);
      }
      return activation;
    },
  });
  return plugin;
}

/** Activate exact factory-created instances in a borrowed app; its owner retains shutdown. */
export async function activateRuntimeApp(
  running: RunningApp,
  plugins: readonly Plugin<unknown>[],
): Promise<void> {
  for (const plugin of plugins) {
    await (plugin as RuntimePlugin)[activateRuntime]?.(running);
  }
}

/** Complete Core setup before publishing explicit Runtime exports. */
export async function startRuntimeApp(
  app: Parameters<typeof startApp>[0],
  options?: Parameters<typeof startApp>[1],
): Promise<RunningApp> {
  const core = await startApp(app, options);
  const runtimes = app.plugins
    .filter((plugin) => (plugin as RuntimePlugin)[activateRuntime])
    .map((plugin) => core.get(plugin as Plugin<RuntimeService<unknown>>));
  let completion: Promise<void> | undefined;
  const running: RunningApp = {
    ...core,
    stop() {
      return (completion ??= Promise.resolve().then(async () => {
        // Drain incoming work before Core releases native services used by handlers.
        // Core reuses these disposers and retains their failures in its cleanup result.
        await Promise.allSettled(runtimes.map((service) => service.disposer?.()));
        await core.stop();
      }));
    },
  };
  try {
    await activateRuntimeApp(running, app.plugins);
    return running;
  } catch (error) {
    try {
      await running.stop();
    } catch (cleanupError) {
      throw new AggregateError(
        [error, cleanupError],
        "Runtime activation failed and application cleanup reported errors.",
        { cause: cleanupError },
      );
    }
    throw error;
  }
}
