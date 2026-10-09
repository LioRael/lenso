import { readFile, realpath, stat } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";

const generated = new Set([".lenso", "dist", ".git", ".turbo", ".wrangler"]);
export const sourceFile = (path: string) => /\.[cm]?[jt]sx?$/.test(path);
export const ignoredInput = (root: string, path: string) =>
  generated.has(relative(root, resolve(path)).split(sep)[0]!);
const contains = (parent: string, path: string) => path === parent || path.startsWith(parent + sep);

function relativeCandidates(path: string): string[] {
  const bases = [path];
  if (/\.[cm]?jsx?$/.test(path)) bases.push(path.replace(/\.[cm]?jsx?$/, ""));
  return bases.flatMap((base) =>
    [
      "",
      ".ts",
      ".tsx",
      ".js",
      ".jsx",
      ".mjs",
      ".cjs",
      ".mts",
      ".cts",
      ".json",
      "/index.ts",
      "/index.tsx",
      "/index.js",
      "/index.mjs",
      "/index.cjs",
      "/package.json",
    ].map((suffix) => base + suffix),
  );
}

function patternStem(pattern: string, value: string): string | undefined {
  const star = pattern.indexOf("*");
  if (star === -1) return pattern === value ? "" : undefined;
  const prefix = pattern.slice(0, star);
  const suffix = pattern.slice(star + 1);
  return value.startsWith(prefix) &&
    value.endsWith(suffix) &&
    value.length >= prefix.length + suffix.length
    ? value.slice(prefix.length, value.length - suffix.length)
    : undefined;
}

function packageEntries(
  root: string,
  metadata: Record<string, unknown> | undefined,
  subpath: string,
): string[] {
  const paths: string[] = [];
  function visit(value: unknown, stem?: string): void {
    if (typeof value === "string" && (!value.includes("*") || stem !== undefined))
      paths.push(resolve(root, stem === undefined ? value : value.replaceAll("*", stem)));
    else if (value && typeof value === "object")
      for (const [key, child] of Object.entries(value)) {
        if (key.startsWith(".")) {
          const matched = patternStem(key, subpath);
          if (matched !== undefined) visit(child, matched);
        } else visit(child, stem);
      }
  }
  if (metadata?.exports !== undefined) visit(metadata.exports);
  else if (subpath !== ".") visit(subpath);
  else
    for (const entry of [metadata?.bun, metadata?.module, metadata?.main, "./index.js"])
      visit(entry);
  return paths;
}

/**
 * Follow Bun's resolved local graph, not every file under the app or its dependencies.
 * Missing probes stay in the graph so creating them can repair a failed import.
 */
export class SourceInputs {
  readonly files = new Set<string>();
  readonly directories = new Set<string>();
  private readonly scanned = new Set<string>();
  private readonly manifests = new Map<
    string,
    { readonly local: ReadonlySet<string>; readonly metadata?: Record<string, unknown> }
  >();
  private readonly configs = new Set<string>();
  private readonly aliases: { scope: string; base: string; pattern: string; targets: string[] }[] =
    [];
  private readonly root: string;
  private canonicalRoot?: string;

  constructor(root: string) {
    this.root = resolve(root);
  }

  private ignored(path: string): boolean {
    return ignoredInput(this.root, path) || ignoredInput(this.canonicalRoot ?? this.root, path);
  }

  async configuration(directory: string): Promise<void> {
    this.canonicalRoot ??= await realpath(this.root);
    for (let current = await realpath(resolve(directory)); ; current = dirname(current)) {
      for (const name of ["package.json", "tsconfig.json", "jsconfig.json", "bunfig.toml"]) {
        const path = join(current, name);
        this.files.add(path);
        if (name === "tsconfig.json" || name === "jsconfig.json") await this.config(path);
      }
      if (dirname(current) === current) break;
    }
  }

  private async config(path: string, scope = dirname(path)): Promise<void> {
    const key = `${scope}\0${path}`;
    if (this.configs.has(key)) return;
    this.configs.add(key);
    this.files.add(path);
    try {
      const config = Bun.JSONC.parse(await readFile(path, "utf8")) as {
        extends?: string | string[];
        compilerOptions?: { baseUrl?: string; paths?: Record<string, string[]> };
      };
      const baseUrl = config.compilerOptions?.baseUrl;
      const aliasBase = resolve(dirname(path), typeof baseUrl === "string" ? baseUrl : ".");
      for (const [pattern, targets] of Object.entries(config.compilerOptions?.paths ?? {}))
        if (Array.isArray(targets))
          this.aliases.push({
            scope,
            base: aliasBase,
            pattern,
            targets: targets.filter((target) => typeof target === "string"),
          });
      const bases = typeof config.extends === "string" ? [config.extends] : (config.extends ?? []);
      for (const base of bases) {
        if (typeof base !== "string") continue;
        let target: string;
        if (base.startsWith(".") || isAbsolute(base)) {
          target = resolve(dirname(path), base);
          if (!target.endsWith(".json")) target += ".json";
        } else target = Bun.resolveSync(base, dirname(path));
        await this.config(target, scope);
      }
    } catch {
      // Config loading belongs to Bun; invalid or missing files remain repair inputs.
    }
  }

  private async packageInputs(directory: string): Promise<void> {
    for (let current = directory; ; current = dirname(current)) {
      if (this.manifests.has(current)) break;
      const manifest = join(current, "package.json");
      this.files.add(manifest);
      const local = new Set<string>();
      let metadata: Record<string, unknown> | undefined;
      try {
        metadata = JSON.parse(await readFile(manifest, "utf8"));
        for (const group of ["dependencies", "devDependencies", "optionalDependencies"])
          for (const [name, value] of Object.entries(metadata?.[group] ?? {}))
            if (typeof value === "string" && /^(workspace:|link:|file:)/.test(value))
              local.add(name);
      } catch {
        // The manifest is still watched; Bun owns validation and actual resolution.
      }
      this.manifests.set(current, { local, ...(metadata ? { metadata } : {}) });
      // A linked package's own tsconfig can affect its imports too.
      await this.config(join(current, "tsconfig.json"));
      if (dirname(current) === current) break;
    }
  }

  private async missing(specifier: string, importer: string): Promise<void> {
    if (specifier.startsWith(".") || isAbsolute(specifier)) {
      const path = resolve(dirname(importer), specifier);
      if (this.ignored(path)) return;
      for (const candidate of relativeCandidates(path)) {
        try {
          if ((await stat(candidate)).isDirectory()) continue;
        } catch {
          // Keep nonexistent file probes.
        }
        this.files.add(candidate);
      }
      return;
    }
    if (specifier.startsWith("node:") || specifier.startsWith("bun:")) return;
    // These are repair candidates only. Existing imports always use Bun.resolveSync.
    for (const alias of this.aliases) {
      if (importer !== alias.scope && !importer.startsWith(alias.scope + sep)) continue;
      const stem = patternStem(alias.pattern, specifier);
      if (stem === undefined) continue;
      for (const target of alias.targets)
        await this.missing(resolve(alias.base, target.replaceAll("*", stem)), importer);
    }
    const parts = specifier.split("/");
    const name = parts[0]!.startsWith("@") ? parts.slice(0, 2).join("/") : parts[0]!;
    for (let current = dirname(importer); ; current = dirname(current)) {
      const packagePath = join(current, "node_modules", name);
      this.files.add(join(packagePath, "package.json"));
      try {
        const canonical = await realpath(packagePath);
        await this.packageInputs(canonical);
        const subpath = "." + specifier.slice(name.length);
        for (const target of packageEntries(
          packagePath,
          this.manifests.get(canonical)?.metadata,
          subpath,
        ))
          await this.missing(target, importer);
      } catch {
        // The runtime reports the failure. The package path remains a repair input.
      }
      if (dirname(current) === current) break;
    }
  }

  async add(entry: string, follow = true): Promise<void> {
    try {
      this.canonicalRoot ??= await realpath(this.root);
    } catch {
      // Missing roots remain repair probes; application loading reports the failure.
    }
    entry = resolve(entry);
    if (this.ignored(entry)) return;
    this.files.add(entry);
    let canonical: string;
    try {
      canonical = await realpath(entry);
      if (!(await stat(canonical)).isFile()) return;
    } catch {
      return;
    }
    if (this.ignored(canonical)) return;
    this.files.add(canonical);
    await this.packageInputs(dirname(canonical));
    if (!follow || this.scanned.has(canonical)) return;
    this.scanned.add(canonical);
    if (!sourceFile(canonical)) return;
    this.directories.add(dirname(canonical));
    let imports;
    try {
      const loader = canonical.endsWith("tsx")
        ? "tsx"
        : canonical.endsWith("jsx")
          ? "jsx"
          : /\.[cm]?ts$/.test(canonical)
            ? "ts"
            : "js";
      imports = new Bun.Transpiler({ loader }).scanImports(await readFile(canonical, "utf8"));
    } catch {
      return;
    }
    for (const item of imports) {
      if (item.path.startsWith("node:") || item.path.startsWith("bun:")) continue;
      let dependencyTree: string | undefined;
      let localDependency = false;
      const lexicalRoutes: string[] = [];
      if (item.path.startsWith(".") || isAbsolute(item.path)) {
        // Retain the lexical route as well as Bun's canonical result to detect link redirects.
        const lexical = resolve(dirname(canonical), item.path);
        if (this.ignored(lexical)) continue;
        lexicalRoutes.push(lexical);
        try {
          this.files.add(
            (await stat(lexical)).isDirectory() ? join(lexical, "package.json") : lexical,
          );
        } catch {
          this.files.add(lexical);
        }
      }
      for (const alias of this.aliases) {
        if (!contains(alias.scope, canonical)) continue;
        const stem = patternStem(alias.pattern, item.path);
        if (stem !== undefined)
          for (const target of alias.targets)
            lexicalRoutes.push(resolve(alias.base, target.replaceAll("*", stem)));
      }
      if (!item.path.startsWith(".") && !isAbsolute(item.path)) {
        const parts = item.path.split("/");
        const name = parts[0]!.startsWith("@") ? parts.slice(0, 2).join("/") : parts[0]!;
        for (let current = dirname(canonical); ; current = dirname(current)) {
          const manifest = join(current, "node_modules", name, "package.json");
          try {
            await stat(manifest);
            this.files.add(manifest);
            dependencyTree = await realpath(join(current, "node_modules"));
            localDependency = this.manifests.get(current)?.local.has(name) === true;
            const packageRoot = dirname(manifest);
            const actualRoot = await realpath(packageRoot);
            await this.packageInputs(actualRoot);
            lexicalRoutes.push(
              ...packageEntries(
                packageRoot,
                this.manifests.get(actualRoot)?.metadata,
                "." + item.path.slice(name.length),
              ),
            );
            break;
          } catch {
            if (dirname(current) === current) break;
          }
        }
      }
      try {
        const file = Bun.resolveSync(item.path, dirname(canonical));
        if (lexicalRoutes.length && isAbsolute(file)) {
          const resolved = await realpath(file);
          const candidates = new Set(lexicalRoutes.flatMap(relativeCandidates));
          for (const route of lexicalRoutes) {
            try {
              if ((await stat(route)).isDirectory()) {
                const actual = await realpath(route);
                if (contains(actual, resolved))
                  candidates.add(resolve(route, relative(actual, resolved)));
              }
            } catch {
              // Extensionless file routes are matched below.
            }
          }
          for (const candidate of candidates) {
            try {
              if ((await realpath(candidate)) === resolved) this.files.add(candidate);
            } catch {
              // Only retain existing routes matching Bun's actual result, not unrelated alternatives.
            }
          }
        }
        // A deliberate link can live under another node_modules tree. Only
        // packages remaining inside that tree without a local declaration are opaque.
        if (isAbsolute(file))
          await this.add(
            file,
            localDependency || !dependencyTree || !contains(dependencyTree, await realpath(file)),
          );
      } catch {
        await this.missing(item.path, canonical);
      }
    }
  }
}
