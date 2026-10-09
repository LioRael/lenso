import { lstatSync, readdirSync, realpathSync, statSync, watch, type FSWatcher } from "node:fs";
import { dirname, relative, resolve, sep } from "node:path";
import { ignoredInput, sourceFile } from "./source-inputs";

type Plan = { recursive: boolean; files: Set<string> };
type Listener = Plan & {
  watcher: FSWatcher;
  observed: Map<string, string | undefined>;
  sourceDirectory: boolean;
};
const contains = (parent: string, path: string) => path === parent || path.startsWith(parent + sep);

function fingerprint(path: string): string | undefined {
  try {
    const stat = lstatSync(path, { bigint: true });
    return `${stat.ino}:${stat.size}:${stat.mtimeNs}:${stat.ctimeNs}`;
  } catch {
    return undefined;
  }
}
function directory(path: string): boolean {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}
function existingParent(path: string): string {
  while (!directory(path) && dirname(path) !== path) path = dirname(path);
  return path;
}

/** One listener per directory; exact graph files and explicit recursive reads share ownership. */
export class InputWatches {
  private readonly listeners = new Map<string, Listener>();
  private readonly canonicalRoot: string;

  constructor(
    private readonly root: string,
    private readonly changed: () => void,
    private readonly failed: (error: unknown) => void,
  ) {
    this.canonicalRoot = realpathSync(root);
  }

  private ignored(path: string): boolean {
    return ignoredInput(this.root, path) || ignoredInput(this.canonicalRoot, path);
  }

  replace(paths: readonly string[], sourceDirectories: readonly string[] = []): void {
    const plans = new Map<string, Plan>();
    const sourceParents = new Set(sourceDirectories);
    const add = (parent: string, file?: string) => {
      const canonical = realpathSync(parent);
      const plan = plans.get(canonical) ?? { recursive: false, files: new Set<string>() };
      if (file === undefined) plan.recursive = true;
      else {
        const canonicalFile = resolve(canonical, relative(parent, file));
        plan.files.add(canonicalFile);
      }
      plans.set(canonical, plan);
    };
    for (let path of new Set(paths)) {
      path = resolve(path);
      if (this.ignored(path)) continue;
      if (directory(path)) add(path);
      else add(existingParent(dirname(path)), path);
      // Observe replacement of a local package symlink as well as its real source.
      for (let parent = path; dirname(parent) !== parent; parent = dirname(parent)) {
        try {
          if (lstatSync(parent).isSymbolicLink()) add(existingParent(dirname(parent)), parent);
        } catch {
          // Missing ancestors are already covered by the nearest existing parent.
        }
      }
    }
    for (const [parent, plan] of plans)
      if (plan.recursive)
        for (const child of plans.keys())
          if (child !== parent && contains(parent, child)) plans.delete(child);
    for (const [parent, listener] of this.listeners)
      if (!plans.has(parent) || plans.get(parent)!.recursive !== listener.recursive) {
        listener.watcher.close();
        this.listeners.delete(parent);
      }
    for (const [parent, plan] of plans) {
      const sourceDirectory = sourceParents.has(parent);
      const current = this.listeners.get(parent);
      if (current) {
        for (const file of plan.files)
          if (!current.files.has(file)) current.observed.set(file, fingerprint(file));
        if (sourceDirectory && !current.sourceDirectory)
          for (const entry of readdirSync(parent))
            if (sourceFile(entry)) {
              const path = resolve(parent, entry);
              current.observed.set(path, fingerprint(path));
            }
        current.files = plan.files;
        current.sourceDirectory = sourceDirectory;
        continue;
      }
      const observed = new Map<string, string | undefined>();
      const baseline = (path: string) => {
        observed.set(path, fingerprint(path));
        let entries;
        try {
          entries = readdirSync(path, { withFileTypes: true });
        } catch (cause) {
          if (
            ["ENOENT", "ENOTDIR", "EACCES", "EPERM"].includes(
              (cause as NodeJS.ErrnoException).code ?? "",
            )
          )
            return;
          throw cause;
        }
        for (const entry of entries) {
          const child = resolve(path, entry.name);
          if (this.ignored(child) || entry.name === "node_modules") continue;
          observed.set(child, fingerprint(child));
          if (entry.isDirectory()) baseline(child);
        }
      };
      if (plan.recursive) baseline(parent);
      else {
        for (const file of plan.files) observed.set(file, fingerprint(file));
        if (sourceDirectory)
          for (const entry of readdirSync(parent))
            if (sourceFile(entry)) {
              const path = resolve(parent, entry);
              observed.set(path, fingerprint(path));
            }
      }
      const listener: Listener = { ...plan, observed, sourceDirectory, watcher: undefined! };
      listener.watcher = watch(parent, { recursive: plan.recursive }, (_event, filename) => {
        const path = filename ? resolve(parent, filename.toString()) : parent;
        if (this.ignored(path)) return;
        const candidates = listener.recursive
          ? filename
            ? [path]
            : [...listener.observed.keys()]
          : [...listener.files].filter(
              (file) => !filename || contains(path, file) || contains(file, path),
            );
        let invalidated = false;
        // A newly created ancestor may still lack the unresolved leaf when its
        // event arrives. Replan at that ancestor rather than waiting for a leaf
        // event a nonrecursive parent listener cannot receive.
        if (!listener.recursive && candidates.length && path !== parent && directory(path)) {
          const value = fingerprint(path);
          if (listener.observed.get(path) !== value) {
            listener.observed.set(path, value);
            invalidated = true;
          }
        }
        // New/deleted modules in a participating local source directory can repair
        // imports. Edits to unrelated existing modules are not graph invalidations.
        if (listener.sourceDirectory && filename && sourceFile(path) && !listener.files.has(path)) {
          const previous = listener.observed.get(path);
          const value = fingerprint(path);
          listener.observed.set(path, value);
          if ((previous === undefined) !== (value === undefined)) this.changed();
        }
        for (const file of candidates) {
          const value = fingerprint(file);
          if (listener.observed.get(file) === value) continue;
          listener.observed.set(file, value);
          invalidated = true;
        }
        if (invalidated) this.changed();
      });
      listener.watcher.on("error", (error) => {
        listener.watcher.close();
        if (this.listeners.get(parent) === listener) this.listeners.delete(parent);
        this.failed(error);
      });
      this.listeners.set(parent, listener);
    }
  }

  close(): void {
    for (const listener of this.listeners.values()) listener.watcher.close();
    this.listeners.clear();
  }
}
