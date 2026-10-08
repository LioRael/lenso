export { defineApp, definePlugin } from "./plugin";
export type { Contribution, Plugin, PluginContext } from "./plugin";
export { DiagnosticError, validatePlugins } from "./diagnostics";
export type { Diagnostic } from "./diagnostics";
export { startApp, lifecycleFailure } from "./lifecycle";
export type { RunningApp, LifecycleFailure } from "./lifecycle";
