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
    if (stat.isDirectory()) return `directory:${stat.ino}:${stat.mode}`;
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
  private reconciliation: ReturnType<typeof setInterval> | undefined;

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

  private snapshot(parent: string, plan: Plan, sourceDirectory: boolean) {
    const observed = new Map<string, string | undefined>();
    const visit = (path: string) => {
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
        if (entry.isDirectory()) visit(child);
      }
    };
    if (plan.recursive) visit(parent);
    for (const file of plan.files) {
      observed.set(file, fingerprint(file));
      for (
        let ancestor = dirname(file);
        ancestor !== parent && contains(parent, ancestor);
        ancestor = dirname(ancestor)
      )
        observed.set(ancestor, fingerprint(ancestor));
    }
    if (sourceDirectory && !plan.recursive) {
      let entries: string[];
      try {
        entries = readdirSync(parent);
      } catch (cause) {
        if (
          !["ENOENT", "ENOTDIR", "EACCES", "EPERM"].includes(
            (cause as NodeJS.ErrnoException).code ?? "",
          )
        )
          throw cause;
        entries = [];
      }
      for (const entry of entries)
        if (sourceFile(entry)) {
          const path = resolve(parent, entry);
          observed.set(path, fingerprint(path));
        }
    }
    return observed;
  }

  private reconcile(eventParent?: string) {
    let invalidated = false;
    for (const [parent, listener] of this.listeners) {
      if (eventParent !== undefined && eventParent !== parent) continue;
      try {
        const next = this.snapshot(parent, listener, listener.sourceDirectory);
        for (const path of new Set([...listener.observed.keys(), ...next.keys()])) {
          const before = listener.observed.get(path);
          const after = next.get(path);
          if (before === after) continue;
          const unownedSource =
            !listener.recursive &&
            listener.sourceDirectory &&
            dirname(path) === parent &&
            sourceFile(path) &&
            !listener.files.has(path);
          const owned = listener.recursive || listener.files.has(path) || next.has(path);
          if (unownedSource ? (before === undefined) !== (after === undefined) : owned)
            invalidated = true;
        }
        listener.observed = next;
      } catch (error) {
        this.failed(error);
      }
    }
    if (invalidated) this.changed();
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
      const observed = this.snapshot(parent, plan, sourceDirectory);
      const listener: Listener = { ...plan, observed, sourceDirectory, watcher: undefined! };
      listener.watcher = watch(parent, { recursive: plan.recursive }, (_event, filename) => {
        const path = filename ? resolve(parent, filename.toString()) : parent;
        if (this.ignored(path)) return;
        this.reconcile(parent);
      });
      listener.watcher.on("error", (error) => {
        listener.watcher.close();
        if (this.listeners.get(parent) === listener) this.listeners.delete(parent);
        if (!this.listeners.size) {
          clearInterval(this.reconciliation);
          this.reconciliation = undefined;
        }
        this.failed(error);
      });
      this.listeners.set(parent, listener);
    }
    // fs.watch can return before its native subscription is active. The same
    // ownership snapshots also reconcile persistent changes when an event is lost.
    if (this.listeners.size && !this.reconciliation) {
      this.reconciliation = setInterval(() => this.reconcile(), 250);
      this.reconciliation.unref();
    } else if (!this.listeners.size) {
      clearInterval(this.reconciliation);
      this.reconciliation = undefined;
    }
  }

  close(): void {
    clearInterval(this.reconciliation);
    this.reconciliation = undefined;
    for (const listener of this.listeners.values()) listener.watcher.close();
    this.listeners.clear();
  }
}
