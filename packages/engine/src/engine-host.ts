import { access, lstat, mkdir, readFile, realpath, unlink } from "node:fs/promises";
import { realpathSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { createHash } from "node:crypto";
import { pathToFileURL } from "node:url";
import { EngineError, diagnostic, type EngineDiagnostic } from "./diagnostics";
import type {
  BuildContext,
  DevEvent,
  EngineConfig,
  EngineContext,
  EngineConvention,
  EngineMode,
  EnginePlugin,
  EngineSnapshot,
  EngineSource,
  GeneratedFile,
  RegistrationOptions,
} from "./engine-authoring";

type Run = (...args: never[]) => unknown;
interface Capability {
  plugin: string;
  source: EngineSource;
  run: Run;
}
interface OwnedFile {
  path: string;
  plugin: string;
  capability: string;
  hash: string;
}
const validName = /^[a-zA-Z0-9@][a-zA-Z0-9@/._-]*$/;
function validSource(source: EngineSource): boolean {
  return (
    !!source &&
    typeof source.file === "string" &&
    !!source.file &&
    (source.export === undefined || typeof source.export === "string") &&
    [source.line, source.column].every(
      (position) => position === undefined || (Number.isInteger(position) && position >= 0),
    )
  );
}
const hash = (content: string) => createHash("sha256").update(content).digest("hex");
function error(
  code: string,
  message: string,
  pluginId: string,
  source: EngineSource,
  phase = "engine-config",
  options?: ErrorOptions,
) {
  return new EngineError({ code, phase, message, pluginId, source }, options);
}
function safeRelative(path: string): boolean {
  return (
    typeof path === "string" &&
    !!path &&
    !isAbsolute(path) &&
    !path.includes("\\") &&
    !path.includes("\0") &&
    path.split("/").every((part) => !!part && part !== "." && part !== "..")
  );
}
/** Guard generated outputs and bundler destinations against lexical and symlink escapes. */
async function outputPath(root: string, path: string): Promise<string> {
  if (!safeRelative(path)) throw new Error("Output path must be a normalized relative path");
  let current = root;
  for (const part of path.split("/")) {
    current = join(current, part);
    try {
      if ((await lstat(current)).isSymbolicLink())
        throw new Error("Output path contains a symbolic link");
    } catch (cause) {
      if ((cause as NodeJS.ErrnoException).code !== "ENOENT") throw cause;
    }
  }
  return current;
}
function appPath(root: string, path: string): string {
  const result = resolve(root, path);
  const local = relative(root, result);
  if (
    !local ||
    local === ".." ||
    local.startsWith(`..${sep}`) ||
    isAbsolute(local) ||
    local.split(sep)[0] === ".lenso" ||
    local.split(sep)[0] === "dist"
  )
    throw new Error("Application sources must be outside generated directories and inside root");
  return result;
}
async function exists(file: string): Promise<boolean> {
  try {
    await access(file);
    return true;
  } catch {
    return false;
  }
}
/** Static imports are invalidation inputs, not discovery of application services. */
async function importInputs(entry: string, inputs: Set<string>, root: string): Promise<void> {
  try {
    entry = await realpath(entry);
  } catch {
    return;
  }
  const local = relative(root, entry).split(sep)[0];
  if ([".lenso", "dist", ".git"].includes(local)) return;
  if (inputs.has(entry) || !(await exists(entry))) return;
  inputs.add(entry);
  if (!/\.[cm]?[jt]sx?$/.test(entry)) return;
  const loader = entry.endsWith("tsx")
    ? "tsx"
    : entry.endsWith("jsx")
      ? "jsx"
      : entry.endsWith("ts")
        ? "ts"
        : "js";
  let imports;
  try {
    imports = new Bun.Transpiler({ loader }).scanImports(await readFile(entry, "utf8"));
  } catch {
    return;
  } // Import itself reports parse errors; dynamic reads require explicit watch().
  for (const item of imports) {
    if (item.kind === "import-statement" && item.path.startsWith("node:")) continue;
    let file;
    try {
      file = Bun.resolveSync(item.path, dirname(entry));
    } catch {
      continue;
    }
    if (isAbsolute(file)) await importInputs(file, inputs, root);
  }
}
function orderPlugins(value: unknown, source: EngineSource): EnginePlugin[] {
  if (!Array.isArray(value))
    throw error("invalid-engine-config", "Engine plugins must be an array.", "engine", source);
  const plugins = value as EnginePlugin[];
  const names = new Map<string, EnginePlugin>();
  for (const plugin of plugins) {
    if (
      !plugin ||
      typeof plugin.name !== "string" ||
      !validName.test(plugin.name) ||
      typeof plugin.setup !== "function"
    )
      throw error(
        "invalid-engine-plugin",
        "Engine plugins require a valid name and setup function.",
        plugin?.name ?? "engine",
        plugin?.source ?? source,
      );
    if (names.has(plugin.name))
      throw error(
        "duplicate-engine-plugin",
        `Duplicate engine plugin "${plugin.name}".`,
        plugin.name,
        plugin.source ?? source,
      );
    if (plugin.source && !validSource(plugin.source))
      throw error("invalid-engine-plugin", "Plugin source requires a file.", plugin.name, source);
    for (const edges of [plugin.before, plugin.after])
      if (
        edges !== undefined &&
        (!Array.isArray(edges) || edges.some((name) => typeof name !== "string"))
      )
        throw error(
          "invalid-engine-order",
          "before/after must be arrays of plugin names.",
          plugin.name,
          plugin.source ?? source,
        );
    names.set(plugin.name, plugin);
  }
  const dependencies = new Map(plugins.map((plugin) => [plugin.name, new Set(plugin.after ?? [])]));
  for (const plugin of plugins)
    for (const next of plugin.before ?? []) {
      if (!names.has(next))
        throw error(
          "missing-engine-order",
          `Unknown ordered plugin "${next}".`,
          plugin.name,
          plugin.source ?? source,
        );
      dependencies.get(next)!.add(plugin.name);
    }
  for (const plugin of plugins)
    for (const prev of dependencies.get(plugin.name)!)
      if (!names.has(prev))
        throw error(
          "missing-engine-order",
          `Unknown ordered plugin "${prev}".`,
          plugin.name,
          plugin.source ?? source,
        );
  const ordered: EnginePlugin[] = [];
  const done = new Set<string>();
  while (ordered.length < plugins.length) {
    const next = plugins.find(
      (plugin) =>
        !done.has(plugin.name) &&
        [...dependencies.get(plugin.name)!].every((name) => done.has(name)),
    );
    if (!next) {
      const plugin = plugins.find((candidate) => !done.has(candidate.name))!;
      throw error(
        "cyclic-engine-order",
        "Engine plugin ordering contains a cycle.",
        plugin.name,
        plugin.source ?? source,
      );
    }
    ordered.push(next);
    done.add(next.name);
  }
  return ordered;
}

export class EngineSession {
  private readonly capabilities = new Map<string, Capability>();
  private readonly cleanups: {
    plugin: string;
    source: EngineSource;
    run: () => void | Promise<void>;
  }[] = [];
  private readonly watches = new Set<string>();
  private readonly watchRegistrations = new Map<string, number>();
  private closed?: Promise<void>;
  private closing = false;
  private cleanupStarted = false;
  private setupPromise?: Promise<void>;
  private setupDefaults?: readonly EnginePlugin[];
  private setupComplete = false;
  private discoveryComplete = false;
  private runningStage?: Promise<unknown>;
  private conventionValue?: EngineConvention;
  private sourceFiles: readonly string[] = [];
  readonly configPath: string;
  target = "bun";
  constructor(
    readonly root: string,
    readonly mode: EngineMode,
  ) {
    this.configPath = join(root, "lenso.engine.ts");
  }
  private async run<T>(capability: Capability, phase: string, ...args: unknown[]): Promise<T> {
    try {
      return (await Reflect.apply(capability.run, undefined, args)) as T;
    } catch (cause) {
      if (cause instanceof EngineError) throw cause;
      throw new EngineError(
        {
          code: "engine-hook-failed",
          phase,
          message: `Engine plugin "${capability.plugin}" failed during ${phase}.`,
          pluginId: capability.plugin,
          source: capability.source,
          causes: [
            diagnostic(cause, { pluginId: capability.plugin, source: capability.source, phase }),
          ],
        },
        { cause },
      );
    }
  }
  setup(defaults: readonly EnginePlugin[]): Promise<void> {
    if (this.closing) return Promise.reject(this.stateError("engine-setup", true));
    if (this.setupPromise) {
      if (
        defaults.length !== this.setupDefaults!.length ||
        defaults.some((plugin, index) => plugin !== this.setupDefaults![index])
      )
        return Promise.reject(this.stateError("engine-setup"));
      return this.setupPromise;
    }
    this.setupDefaults = [...defaults];
    return (this.setupPromise = Promise.resolve().then(async () => {
      await this.performSetup(this.setupDefaults!);
      this.setupComplete = true;
    }));
  }
  private stateError(phase: string, closed = false): EngineError {
    return error(
      closed ? "engine-session-closed" : "invalid-engine-state",
      closed
        ? "Engine session is closing or closed."
        : "Engine stage prerequisites are not satisfied.",
      "engine",
      { file: this.configPath },
      phase,
    );
  }
  // Processing stages are serial, not queued. Hooks must not await close(), which waits for them.
  private stage<T>(phase: string, needsDiscovery: boolean, run: () => Promise<T>): Promise<T> {
    if (this.closing) return Promise.reject(this.stateError(phase, true));
    if (!this.setupComplete || (needsDiscovery && !this.discoveryComplete) || this.runningStage)
      return Promise.reject(this.stateError(phase));
    const pending = Promise.resolve().then(run);
    const tracked = pending.finally(() => {
      this.runningStage = undefined;
    });
    this.runningStage = tracked;
    return tracked;
  }
  private async performSetup(defaults: readonly EnginePlugin[]): Promise<void> {
    let config: EngineConfig = {};
    if (await exists(this.configPath)) {
      try {
        config = (await import(pathToFileURL(this.configPath).href)).default;
      } catch (cause) {
        throw new EngineError(
          {
            code: "engine-config-load-failed",
            phase: "engine-config",
            message: "Cannot load trusted lenso.engine.ts.",
            source: { file: this.configPath },
          },
          { cause },
        );
      }
      if (
        !config ||
        typeof config !== "object" ||
        Object.keys(config).some((key) => !["plugins", "target"].includes(key)) ||
        (config.target !== undefined &&
          (typeof config.target !== "string" || !validName.test(config.target)))
      )
        throw error(
          "invalid-engine-config",
          "Engine config requires plugins and an optional target name.",
          "engine",
          { file: this.configPath },
        );
      await importInputs(this.configPath, this.watches, await realpath(this.root));
    }
    if (config.plugins !== undefined && !Array.isArray(config.plugins))
      throw error("invalid-engine-config", "Engine plugins must be an array.", "engine", {
        file: this.configPath,
      });
    const plugins = orderPlugins([...defaults, ...(config.plugins ?? [])], {
      file: this.configPath,
    });
    this.target = config.target ?? "bun";
    for (const plugin of plugins) {
      const source = plugin.source ?? { file: this.configPath, export: plugin.name };
      let active = true;
      const ensureActive = () => {
        if (!active)
          throw error(
            "late-engine-registration",
            "Registration is only valid during setup.",
            plugin.name,
            source,
          );
      };
      const register = (
        kind: string,
        name: string,
        run: Run,
        options: RegistrationOptions = {},
      ) => {
        ensureActive();
        const location = options.source ?? source;
        if (
          !validSource(location) ||
          (options.replace !== undefined && typeof options.replace !== "string")
        )
          throw error(
            "invalid-engine-capability",
            "Capability source and replacement owner must be valid.",
            plugin.name,
            source,
          );
        if (typeof name !== "string" || !validName.test(name) || typeof run !== "function")
          throw error(
            "invalid-engine-capability",
            "Capability requires a name and function.",
            plugin.name,
            location,
          );
        const key = `${kind}:${name}`;
        const previous = this.capabilities.get(key);
        if (previous ? options.replace !== previous.plugin : options.replace !== undefined)
          throw error(
            "engine-capability-conflict",
            previous
              ? `Capability "${key}" belongs to "${previous.plugin}"; use replace with that exact owner.`
              : `Cannot replace missing capability "${key}".`,
            plugin.name,
            location,
          );
        const capability = { plugin: plugin.name, source: location, run };
        this.capabilities.set(key, capability);
        let revoked = false;
        const revoke = () => {
          if (revoked) return;
          revoked = true;
          // A replaced owner's handle must never remove its successor.
          if (this.capabilities.get(key) === capability) this.capabilities.delete(key);
        };
        this.cleanups.push({ plugin: plugin.name, source: location, run: revoke });
        return revoke;
      };
      const context = Object.freeze<EngineContext>({
        root: this.root,
        mode: this.mode,
        convention: (run, options) => register("convention", "app", run as Run, options),
        discover: (name, run, options) => register("discover", name, run as Run, options),
        generate: (name, run, options) => register("generate", name, run as Run, options),
        target: (name, run, options) => register("target", name, run as Run, options),
        dev: (name, run, options) => register("dev", name, run as Run, options),
        watch: (path) => {
          if (this.cleanupStarted)
            throw error(
              "late-engine-registration",
              "Engine session is closing.",
              plugin.name,
              source,
            );
          if (typeof path !== "string" || !path)
            throw error("invalid-engine-watch", "Watch requires a path.", plugin.name, source);
          let absolute;
          try {
            absolute = realpathSync(resolve(this.root, path));
          } catch (cause) {
            throw error(
              "invalid-engine-watch",
              "Watch input must exist.",
              plugin.name,
              source,
              "engine-config",
              { cause },
            );
          }
          const root = realpathSync(this.root);
          const local = relative(root, absolute).split(sep)[0];
          if (
            [".lenso", "dist", ".git"].includes(local) ||
            root === absolute ||
            root.startsWith(`${absolute}${sep}`)
          )
            throw error(
              "invalid-engine-watch",
              "Generated output and Git metadata cannot be watched.",
              plugin.name,
              source,
            );
          this.watchRegistrations.set(absolute, (this.watchRegistrations.get(absolute) ?? 0) + 1);
          let revoked = false;
          const revoke = () => {
            if (revoked) return;
            revoked = true;
            const remaining = this.watchRegistrations.get(absolute)! - 1;
            if (remaining) this.watchRegistrations.set(absolute, remaining);
            else this.watchRegistrations.delete(absolute);
          };
          this.cleanups.push({ plugin: plugin.name, source, run: revoke });
          return revoke;
        },
        onCleanup: (run) => {
          if (this.cleanupStarted)
            throw error(
              "late-engine-registration",
              "Engine session is closing.",
              plugin.name,
              source,
            );
          if (typeof run !== "function")
            throw error(
              "invalid-engine-cleanup",
              "Cleanup must be a function.",
              plugin.name,
              source,
            );
          let completion: Promise<void> | undefined;
          const dispose = () => (completion ??= Promise.resolve().then(run));
          this.cleanups.push({ plugin: plugin.name, source, run: dispose });
          return dispose;
        },
      });
      try {
        await this.run<void>(
          { plugin: plugin.name, source, run: plugin.setup as Run },
          "engine-setup",
          context,
        );
      } finally {
        active = false;
      }
    }
    if (!this.capabilities.has(`target:${this.target}`))
      throw error("unknown-engine-target", `Unknown build target "${this.target}".`, "engine", {
        file: this.configPath,
      });
  }
  discover(): Promise<EngineSnapshot> {
    return this.stage("engine-discovery", false, async () => {
      this.discoveryComplete = false;
      const snapshot = await this.performDiscovery();
      this.discoveryComplete = true;
      return snapshot;
    });
  }
  private async performDiscovery(): Promise<EngineSnapshot> {
    const capability = this.capabilities.get("convention:app");
    if (!capability) throw this.stateError("engine-discovery");
    const convention = await this.run<EngineConvention>(capability, "engine-discovery");
    try {
      if (
        !convention ||
        typeof convention.config !== "string" ||
        [convention.entry, convention.router].some(
          (path) => path !== undefined && typeof path !== "string",
        )
      )
        throw new Error("Invalid convention");
      for (const path of [convention.config, convention.entry, convention.router])
        if (path) appPath(this.root, path);
    } catch (cause) {
      throw error(
        "invalid-engine-convention",
        "Convention paths must be application sources inside root.",
        capability.plugin,
        capability.source,
        "engine-config",
        { cause },
      );
    }
    this.conventionValue = Object.freeze({
      config: convention.config,
      ...(convention.entry ? { entry: convention.entry } : {}),
      ...(convention.router ? { router: convention.router } : {}),
    });
    await importInputs(
      appPath(this.root, convention.config),
      this.watches,
      await realpath(this.root),
    );
    const sources = new Set<string>();
    for (const path of [convention.config, convention.entry, convention.router])
      if (path && (await exists(appPath(this.root, path)))) {
        sources.add(appPath(this.root, path));
        await importInputs(appPath(this.root, path), this.watches, await realpath(this.root));
      }
    this.sourceFiles = [...sources];
    for (const [key, hook] of this.capabilities)
      if (key.startsWith("discover:")) {
        const paths = await this.run<readonly string[]>(hook, "engine-discovery", this.snapshot());
        if (!Array.isArray(paths) || paths.some((path) => typeof path !== "string"))
          throw error(
            "invalid-engine-sources",
            "Discovery must return source paths.",
            hook.plugin,
            hook.source,
            "engine-discovery",
          );
        for (const path of paths) {
          let absolute;
          try {
            absolute = appPath(this.root, path);
            await access(absolute);
          } catch (cause) {
            throw error(
              "invalid-engine-sources",
              "Discovered sources must exist inside application root.",
              hook.plugin,
              hook.source,
              "engine-discovery",
              { cause },
            );
          }
          sources.add(absolute);
          await importInputs(absolute, this.watches, await realpath(this.root));
        }
        this.sourceFiles = [...sources];
      }
    return this.snapshot();
  }
  snapshot(): EngineSnapshot {
    if (!this.conventionValue) throw this.stateError("engine-snapshot");
    const root = this.root;
    return Object.freeze({
      root,
      mode: this.mode,
      convention: this.conventionValue,
      sources: Object.freeze([...this.sourceFiles]),
      watchFiles: Object.freeze(
        [...new Set([...this.watches, ...this.watchRegistrations.keys()])].sort(),
      ),
      importPath(output: string, source: string) {
        if (!safeRelative(output)) throw new Error("Generated path must be relative to .lenso");
        const target = appPath(root, source);
        const specifier = relative(dirname(join(root, ".lenso", output)), target)
          .replaceAll("\\", "/")
          .replace(/\.(?:tsx?|jsx?)$/, "");
        return specifier.startsWith(".") ? specifier : `./${specifier}`;
      },
    });
  }
  generate(): Promise<void> {
    return this.stage("engine-generation", true, () => this.performGeneration());
  }
  private async performGeneration(): Promise<void> {
    const files: (OwnedFile & { content: string; source: EngineSource })[] = [];
    const paths = new Set<string>();
    for (const [key, hook] of this.capabilities)
      if (key.startsWith("generate:")) {
        const generated = await this.run<readonly GeneratedFile[]>(
          hook,
          "engine-generation",
          this.snapshot(),
        );
        if (!Array.isArray(generated))
          throw error(
            "invalid-generated-file",
            "Generator must return files.",
            hook.plugin,
            hook.source,
            "engine-generation",
          );
        for (const file of generated) {
          if (
            !file ||
            !safeRelative(file.path) ||
            file.path.toLowerCase() === ".engine-files.json" ||
            typeof file.content !== "string"
          )
            throw error(
              "invalid-generated-file",
              "Generated paths must be normalized paths relative to .lenso.",
              hook.plugin,
              hook.source,
              "engine-generation",
            );
          if (
            [...paths].some(
              (path) =>
                path.toLowerCase() === file.path.toLowerCase() ||
                path.toLowerCase().startsWith(`${file.path.toLowerCase()}/`) ||
                file.path.toLowerCase().startsWith(`${path.toLowerCase()}/`),
            )
          )
            throw error(
              "generated-file-conflict",
              `Generated file "${file.path}" has multiple owners. Replace the generator explicitly.`,
              hook.plugin,
              hook.source,
              "engine-generation",
            );
          paths.add(file.path);
          files.push({
            ...file,
            plugin: hook.plugin,
            capability: key,
            hash: hash(file.content),
            source: hook.source,
          });
        }
      }
    const ownershipPath = await this.checkedOutput(".lenso/.engine-files.json", "engine", {
      file: this.configPath,
    });
    let previous: OwnedFile[] = [];
    if (await exists(ownershipPath)) {
      try {
        const loaded = JSON.parse(await readFile(ownershipPath, "utf8"));
        if (
          loaded.schemaVersion !== 1 ||
          !Array.isArray(loaded.files) ||
          loaded.files.some(
            (file: OwnedFile) =>
              !file ||
              !safeRelative(file.path) ||
              file.path.toLowerCase() === ".engine-files.json" ||
              typeof file.hash !== "string" ||
              typeof file.plugin !== "string" ||
              typeof file.capability !== "string",
          )
        )
          throw new Error("Invalid ownership");
        previous = loaded.files;
        for (const file of files)
          if (
            previous.some(
              (old) => old.path !== file.path && old.path.toLowerCase() === file.path.toLowerCase(),
            )
          )
            throw new Error("Case-only output rename");
      } catch (cause) {
        throw error(
          "invalid-generated-ownership",
          "Generated ownership metadata is invalid; do not edit .lenso output.",
          "engine",
          { file: ownershipPath },
          "engine-generation",
          { cause },
        );
      }
    }
    const outputs = new Map<string, string>();
    for (const file of [...previous, ...files]) {
      const path = await this.checkedOutput(
        `.lenso/${file.path}`,
        file.plugin,
        "source" in file ? (file.source as EngineSource) : { file: ownershipPath },
      );
      outputs.set(file.path, path);
      const old = previous.find((item) => item.path === file.path);
      if (await exists(path)) {
        const content = await readFile(path, "utf8");
        if (!old || hash(content) !== old.hash)
          throw error(
            "generated-file-modified",
            "Generated output has unowned or edited content. Move it out of .lenso before regenerating.",
            file.plugin,
            { file: path },
            "engine-generation",
          );
      }
    }
    for (const file of files) {
      const path = outputs.get(file.path)!;
      await mkdir(dirname(path), { recursive: true });
      if (!(await exists(path)) || (await readFile(path, "utf8")) !== file.content)
        await Bun.write(path, file.content);
    }
    for (const file of previous)
      if (!paths.has(file.path) && (await exists(outputs.get(file.path)!)))
        await unlink(outputs.get(file.path)!);
    await mkdir(dirname(ownershipPath), { recursive: true });
    const content = `${JSON.stringify({ schemaVersion: 1, files: files.map(({ path, plugin, capability, hash: fileHash }) => ({ path, plugin, capability, hash: fileHash })) }, null, 2)}\n`;
    if (!(await exists(ownershipPath)) || (await readFile(ownershipPath, "utf8")) !== content)
      await Bun.write(ownershipPath, content);
  }
  private async checkedOutput(path: string, plugin: string, source: EngineSource): Promise<string> {
    try {
      return await outputPath(this.root, path);
    } catch (cause) {
      throw error(
        "unsafe-engine-output",
        "Output escapes its owned directory or contains a symbolic link.",
        plugin,
        source,
        "engine-generation",
        { cause },
      );
    }
  }
  build(entry?: string): Promise<string> {
    return this.stage("build", true, () => this.performBuild(entry));
  }
  private async performBuild(entry?: string): Promise<string> {
    const key = `target:${this.target}`;
    const unavailable = () =>
      error(
        "unknown-engine-target",
        `Unknown build target "${this.target}".`,
        "engine",
        {
          file: this.configPath,
        },
        "build",
      );
    const hook = this.capabilities.get(key);
    if (!hook) throw unavailable();
    const chosen = entry ?? this.conventionValue!.entry ?? ".lenso/server.ts";
    const entryPath = resolve(this.root, chosen);
    if (relative(this.root, entryPath).startsWith("..") || !(await exists(entryPath)))
      throw error(
        "invalid-build-entry",
        "Build entry must exist inside application root.",
        hook.plugin,
        hook.source,
        "build",
      );
    const context = Object.freeze<BuildContext>({
      ...this.snapshot(),
      entry: entryPath,
      bundle: async (options) => {
        const output = options.directory ? `dist/${options.directory}` : "dist";
        const outdir = await this.checkedOutput(output, hook.plugin, hook.source);
        const bundledEntry = resolve(this.root, options.entry);
        const local = relative(this.root, bundledEntry);
        if (!local || local === ".." || local.startsWith(`..${sep}`) || isAbsolute(local))
          throw error(
            "invalid-build-entry",
            "Bundle entry must be inside application root.",
            hook.plugin,
            hook.source,
            "build",
          );
        const result = await Bun.build({
          entrypoints: [bundledEntry],
          outdir,
          target: options.platform ?? "bun",
          packages: options.packages ?? "external",
          sourcemap: "external",
        });
        if (!result.success)
          throw new EngineError(
            {
              code: "build-failed",
              phase: "build",
              message: "Application build failed.",
              pluginId: hook.plugin,
              source: hook.source,
              causes: result.logs.map((log) => ({
                code: "build-diagnostic",
                phase: "build",
                message: "Bun build diagnostic; inspect the source location.",
                source: log.position
                  ? {
                      file: log.position.file,
                      line: log.position.line,
                      column: log.position.column,
                    }
                  : { file: bundledEntry },
              })),
            },
            { cause: new AggregateError(result.logs, "Application build failed.") },
          );
        return outdir;
      },
    });
    // Entry validation yields; a target revoked before invocation must stay unavailable.
    if (this.capabilities.get(key) !== hook) throw unavailable();
    const outdir = await this.run<string>(hook, "build", context);
    if (typeof outdir !== "string")
      throw error(
        "invalid-build-output",
        "Build target must return a directory inside dist.",
        hook.plugin,
        hook.source,
        "build",
      );
    const local = relative(join(this.root, "dist"), resolve(outdir));
    if (local === ".." || local.startsWith(`..${sep}`) || isAbsolute(local))
      throw error(
        "invalid-build-output",
        "Build target must return a directory inside dist.",
        hook.plugin,
        hook.source,
        "build",
      );
    if (!(await exists(outdir)) || !(await lstat(outdir)).isDirectory())
      throw error(
        "invalid-build-output",
        "Build target returned a missing output directory.",
        hook.plugin,
        hook.source,
        "build",
      );
    return outdir;
  }
  dev(event: DevEvent): Promise<void> {
    return this.stage("engine-dev", true, async () => {
      for (const [key, hook] of this.capabilities)
        if (key.startsWith("dev:"))
          await this.run<void>(hook, "engine-dev", event, this.snapshot());
    });
  }
  close(): Promise<void> {
    this.closing = true;
    return (this.closed ??= Promise.resolve().then(async () => {
      // Setup/stage failures remain on their own promises; cleanup must still run.
      await this.setupPromise?.catch(() => {});
      await this.runningStage?.catch(() => {});
      this.cleanupStarted = true;
      const causes: EngineDiagnostic[] = [];
      const errors: unknown[] = [];
      for (const cleanup of this.cleanups.splice(0).reverse()) {
        try {
          await cleanup.run();
        } catch (cause) {
          errors.push(cause);
          causes.push(
            diagnostic(cause, {
              code: "engine-cleanup-failed",
              phase: "engine-cleanup",
              message: `Engine plugin "${cleanup.plugin}" cleanup failed.`,
              pluginId: cleanup.plugin,
              source: cleanup.source,
            }),
          );
        }
      }
      this.capabilities.clear();
      this.watches.clear();
      this.watchRegistrations.clear();
      this.sourceFiles = [];
      if (causes.length)
        throw new EngineError(
          {
            code: "engine-cleanup-failed",
            phase: "engine-cleanup",
            message: "Engine resource cleanup failed.",
            causes,
          },
          { cause: new AggregateError(errors, "Engine resource cleanup failed.") },
        );
    }));
  }
}
export async function withEngine<T>(session: EngineSession, run: () => Promise<T>): Promise<T> {
  let result: T | undefined;
  let failure: unknown;
  let failed = false;
  try {
    result = await run();
  } catch (cause) {
    failed = true;
    failure = cause;
  }
  try {
    await session.close();
  } catch (cause) {
    if (!failed) throw cause;
    throw new EngineError(
      {
        code: "engine-and-cleanup-failed",
        phase: "engine-cleanup",
        message: "Engine execution and cleanup failed.",
        causes: [diagnostic(failure), diagnostic(cause)],
      },
      { cause: new AggregateError([failure, cause], "Engine execution and cleanup failed.") },
    );
  }
  if (failed) throw failure;
  return result as T;
}
