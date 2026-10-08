export { defineApp, definePlugin } from "./plugin";
export type { Cleanup, Contribution, Plugin, PluginContext, PluginSource } from "./plugin";
export { DiagnosticError, validatePlugins } from "./diagnostics";
export type { Diagnostic } from "./diagnostics";
export { startApp, lifecycleFailure } from "./lifecycle";
export type { RunningApp, LifecycleFailure } from "./lifecycle";
export {
  definePluginConfig,
  bindConfig,
  valuesSource,
  resolveConfig,
  preflightConfigs,
  ConfigError,
  ConfigSourceError,
} from "./config";
export type * from "./config-types";
