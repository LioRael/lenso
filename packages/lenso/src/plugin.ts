import type { StandardSchemaV1 } from "@standard-schema/spec";
import type { ConfigBinding } from "./config-types";

export type Cleanup = () => Promise<void>;

export interface Logger {
  child(bindings: Record<string, unknown>): Logger;
  debug(fields: Record<string, unknown>, message?: string): void;
  info(fields: Record<string, unknown>, message?: string): void;
  warn(fields: Record<string, unknown>, message?: string): void;
  error(fields: Record<string, unknown>, message?: string): void;
}

export interface PluginSource {
  readonly file: string;
  readonly export?: string;
  readonly line?: number;
  readonly column?: number;
}

/** A resource acquired during setup must register its cleanup immediately. */
export interface PluginContext {
  readonly instanceId: string;
  readonly logger?: Logger;
  get<T>(dependency: Plugin<T>): T;
  config<S extends StandardSchemaV1>(binding: ConfigBinding<S>): StandardSchemaV1.InferOutput<S>;
  /** The returned disposer shares its completion with automatic LIFO cleanup. */
  onCleanup(cleanup: () => void | Promise<void>): Cleanup;
}

/** IDs identify instances; use distinct IDs for multiple instances of a plugin. */
export interface Plugin<T = unknown> {
  readonly id: string;
  readonly source?: PluginSource;
  readonly requires?: readonly Plugin<unknown>[];
  readonly contributions?: readonly Contribution[];
  readonly config?: ConfigBinding;
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
  instanceId?: string;
  logger?: Logger;
}): { plugins: P; instanceId?: string; logger?: Logger } {
  return app;
}
