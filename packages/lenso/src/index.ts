export { defineApp, definePlugin } from "./plugin";
export { collectPlugins } from "./assembly";
export type {
  Cleanup,
  ConfiguredPluginContext,
  Contribution,
  Logger,
  Plugin,
  PluginContext,
  PluginSource,
} from "./plugin";
export { DiagnosticError, validatePlugins, validatePluginSelection } from "./diagnostics";
export type { Diagnostic } from "./diagnostics";
export { startApp, withApp, lifecycleFailure } from "./lifecycle";
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
