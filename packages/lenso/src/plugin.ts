/** A resource acquired during setup must register its cleanup immediately. */
export interface PluginContext {
  get<T>(dependency: Plugin<T>): T;
  onCleanup(cleanup: () => void | Promise<void>): void;
}

/** IDs identify instances; use distinct IDs for multiple instances of a plugin. */
export interface Plugin<T = unknown> {
  readonly id: string;
  readonly requires?: readonly Plugin<unknown>[];
  readonly contributions?: readonly Contribution[];
  readonly setup: (context: PluginContext) => T | Promise<T>;
}

export interface Contribution {
  readonly kind: string;
  readonly [key: string]: unknown;
}

export function definePlugin<T>(plugin: Plugin<T>): Plugin<T> {
  return plugin;
}

export function defineApp<const P extends readonly Plugin<unknown>[]>(app: {
  plugins: P;
}): { plugins: P } {
  return app;
}
