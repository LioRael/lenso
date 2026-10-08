/** Build-time API. Import this subpath only from lenso.engine.ts or build plugins. */
export interface EngineSource {
  readonly file: string;
  readonly export?: string;
  readonly line?: number;
  readonly column?: number;
}
export type EngineMode = "check" | "generate" | "build" | "dev";
export type DevEvent = "beforeStart" | "ready";
export interface EngineConvention {
  /** Application assembly, never the engine configuration. Relative to root. */
  readonly config: string;
  readonly entry?: string;
  readonly router?: string;
}
export interface GeneratedFile {
  /** Relative to .lenso. Each output has exactly one active generator owner. */
  readonly path: string;
  readonly content: string;
}
export interface BundleOptions {
  readonly entry: string;
  readonly platform?: "bun" | "browser" | "node";
  readonly packages?: "bundle" | "external";
  /** Relative path inside dist. */
  readonly directory?: string;
}
export interface EngineSnapshot {
  readonly root: string;
  readonly mode: EngineMode;
  readonly convention: EngineConvention;
  readonly sources: readonly string[];
  /** Paths to watch, including explicit directories for detecting newly added files. */
  readonly watchFiles: readonly string[];
  /** Static module specifier from an output relative to .lenso to an application source. */
  importPath(output: string, source: string): string;
}
export interface BuildContext extends EngineSnapshot {
  readonly entry: string;
  /** Shared Bun bundling implementation; output is restricted to dist. */
  bundle(options: BundleOptions): Promise<string>;
}
export interface RegistrationOptions {
  /** Exact current owner name. Replacement is never inferred from registration order. */
  readonly replace?: string;
  readonly source?: EngineSource;
}
/** Synchronously revoke this registration. Repeated calls are harmless. */
export type Registration = () => void;
/** Release once; explicit calls and session close share the same completion. */
export type Cleanup = () => Promise<void>;
export interface EngineContext {
  readonly root: string;
  readonly mode: EngineMode;
  convention(
    run: () => EngineConvention | Promise<EngineConvention>,
    options?: RegistrationOptions,
  ): Registration;
  discover(
    name: string,
    run: (context: EngineSnapshot) => readonly string[] | Promise<readonly string[]>,
    options?: RegistrationOptions,
  ): Registration;
  generate(
    name: string,
    run: (context: EngineSnapshot) => readonly GeneratedFile[] | Promise<readonly GeneratedFile[]>,
    options?: RegistrationOptions,
  ): Registration;
  target(
    name: string,
    run: (context: BuildContext) => string | Promise<string>,
    options?: RegistrationOptions,
  ): Registration;
  dev(
    name: string,
    /** Awaited for completion; return values are ignored, never vetoes. */
    run: (event: DevEvent, context: EngineSnapshot) => unknown,
    options?: RegistrationOptions,
  ): Registration;
  /** File/directory reads not expressed as static imports must be registered explicitly. */
  watch(path: string): Registration;
  /** Register immediately after acquiring a resource, including inside hooks via this context. Sequential LIFO; runs on all exits. */
  onCleanup(cleanup: () => void | Promise<void>): Cleanup;
}
export interface EnginePlugin {
  readonly name: string;
  readonly source?: EngineSource;
  readonly before?: readonly string[];
  readonly after?: readonly string[];
  /** Awaited for completion; return values are ignored, including registration handles. */
  setup(context: EngineContext): unknown;
}
export interface EngineConfig {
  readonly plugins?: readonly EnginePlugin[];
  readonly target?: string;
}
export function defineEnginePlugin<const T extends EnginePlugin>(plugin: T): T {
  return plugin;
}
export function defineEngineConfig<const T extends EngineConfig>(config: T): T {
  return config;
}
