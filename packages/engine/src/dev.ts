import { watch, realpathSync, readdirSync, statSync, type FSWatcher } from "node:fs";
import { resolve, relative, extname, sep } from "node:path";
import { isDevReadyMessage } from "./dev-ready";
import { devConditionArgs, startEngineDevCycle, type EngineDevCycle } from "./engine-dev";
import { diagnostic, EngineError, type EngineDiagnostic } from "./diagnostics";
import type { ApplicationTarget } from "./application";
import { checkedEntry } from "./engine-host";

export type DevSupervisorEvent =
  | { type: "starting" }
  | { type: "ready"; urls?: readonly string[]; capabilities?: readonly string[] }
  | { type: "failed"; diagnostic: EngineDiagnostic }
  | { type: "exited"; code: number };

export interface DevSupervisorOptions extends ApplicationTarget {
  entry?: string;
  /** Observers cannot interrupt resource ownership; synchronous exceptions are ignored. */
  onEvent?(event: DevSupervisorEvent): void;
  stdout?: "inherit" | "ignore";
  stderr?: "inherit" | "ignore";
}

export interface DevSupervisor {
  /** Resolves after shutdown, including when close rejects with a cleanup failure. */
  readonly done: Promise<void>;
  /** Idempotent; rejects with any cleanup failures observed over the supervisor lifetime. */
  close(): Promise<void>;
}

function includesCleanup(detail: EngineDiagnostic): boolean {
  return (
    detail.phase === "engine-cleanup" ||
    detail.phase === "cleanup" ||
    (detail.causes?.some(includesCleanup) ?? false)
  );
}

async function stopProcess(previous: ReturnType<typeof Bun.spawn>) {
  if (previous.exitCode !== null) return;
  let forced = false;
  previous.kill("SIGTERM");
  const timeout = setTimeout(() => {
    forced = true;
    previous.kill("SIGKILL");
  }, 5000);
  try {
    await previous.exited;
    if (forced)
      throw new EngineError({
        code: "dev-runtime-timeout",
        phase: "engine-cleanup",
        message: "Development runtime shutdown exceeded 5 seconds; the process was terminated.",
      });
  } finally {
    clearTimeout(timeout);
  }
}

export async function createDevSupervisor(options: DevSupervisorOptions): Promise<DevSupervisor> {
  const root = realpathSync(resolve(options.root));
  type Active = {
    child: ReturnType<typeof Bun.spawn>;
    engine: EngineDevCycle;
    ready: boolean;
  };
  let active: Active | undefined;
  let stopping = Promise.resolve();
  let closed = false;
  let queued = false;
  let restarting: Promise<void> | undefined;
  let closing: Promise<void> | undefined;
  let debounce: ReturnType<typeof setTimeout> | undefined;
  let finish!: () => void;
  const done = new Promise<void>((complete) => {
    finish = complete;
  });
  const cleanupFailures: unknown[] = [];
  const watchers = new Map<string, FSWatcher>();
  const ignored = new Set([".lenso", "dist", "node_modules", ".git", ".turbo", ".wrangler"]);

  function emit(event: DevSupervisorEvent) {
    try {
      options.onEvent?.(event);
    } catch {
      // Observers do not own the supervisor lifecycle.
    }
  }
  function report(error: unknown) {
    emit({ type: "failed", diagnostic: diagnostic(error) });
  }
  function cleanupFailed(error: unknown) {
    cleanupFailures.push(error);
    report(error);
  }
  function stopActive(): Promise<void> {
    const previous = active;
    active = undefined;
    if (previous)
      stopping = stopping.then(async () => {
        try {
          await stopProcess(previous.child);
        } catch (error) {
          cleanupFailed(error);
        }
        try {
          await previous.engine.close();
        } catch (error) {
          cleanupFailed(error);
        }
      });
    return stopping;
  }
  function changed() {
    if (closed) return;
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(() => {
      void restart().catch(report);
    }, 100);
  }
  function watchPath(watchedPath: string, sourceOnly = false) {
    const canonical = realpathSync(watchedPath);
    if (watchers.has(canonical)) return;
    const directory = statSync(canonical).isDirectory();
    const sourceExtensions = [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".json"];
    const observed = new Map<string, string>();
    function fingerprint(path: string): string | undefined {
      try {
        const stat = statSync(path, { bigint: true });
        return `${stat.ino}:${stat.size}:${stat.mtimeNs}:${stat.ctimeNs}`;
      } catch {
        return undefined;
      }
    }
    function remember(path: string) {
      const value = fingerprint(path);
      if (value !== undefined) observed.set(path, value);
    }
    function baseline(path: string) {
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
      for (const directoryEntry of entries) {
        if (ignored.has(directoryEntry.name)) continue;
        const child = resolve(path, directoryEntry.name);
        if (directoryEntry.isDirectory()) {
          if (!sourceOnly) remember(child);
          baseline(child);
        } else if (!sourceOnly || sourceExtensions.includes(extname(child))) remember(child);
      }
    }
    if (directory) {
      if (!sourceOnly) remember(canonical);
      baseline(canonical);
    } else remember(canonical);
    const watcher = watch(canonical, { recursive: directory }, (_event, filename) => {
      const changedPath = filename
        ? resolve(directory ? canonical : resolve(canonical, ".."), filename.toString())
        : canonical;
      if (
        relative(root, changedPath)
          .split(sep)
          .some((part) => ignored.has(part))
      )
        return;
      if (sourceOnly && !sourceExtensions.includes(extname(changedPath))) return;
      // macOS can deliver pre-watch writes late; unchanged input is not invalidation.
      const current = fingerprint(changedPath);
      if (current === observed.get(changedPath)) return;
      if (current === undefined) observed.delete(changedPath);
      else observed.set(changedPath, current);
      changed();
    });
    watcher.on("error", report);
    watchers.set(canonical, watcher);
  }
  function resetWatches(paths: readonly string[] = []) {
    for (const watcher of watchers.values()) watcher.close();
    watchers.clear();
    if (closed) return;
    // Observe newly added configs/imports even after a failed build.
    watchPath(root, true);
    for (const path of paths) watchPath(path);
  }
  function restart(): Promise<void> {
    queued = true;
    if (restarting) return restarting;
    if (closed) return Promise.resolve();
    restarting = (async () => {
      while (queued && !closed) {
        queued = false;
        await stopActive();
        if (closed) break;
        emit({ type: "starting" });
        resetWatches();
        let engine: EngineDevCycle;
        try {
          engine = await startEngineDevCycle({ root, config: options.config }, options.entry);
        } catch (error) {
          if (includesCleanup(diagnostic(error))) cleanupFailures.push(error);
          report(error);
          continue;
        }
        if (closed) {
          try {
            await engine.close();
          } catch (error) {
            cleanupFailed(error);
          }
          break;
        }
        try {
          resetWatches(engine.watchFiles);
          const entry = await checkedEntry(root, engine.entry, "dev");
          const child = Bun.spawn([process.execPath, ...devConditionArgs(), entry], {
            cwd: root,
            stdout: options.stdout ?? "inherit",
            stderr: options.stderr ?? "inherit",
            ipc(message, subprocess) {
              const current = active;
              if (
                closed ||
                !current ||
                current.child !== subprocess ||
                current.ready ||
                !isDevReadyMessage(message)
              )
                return;
              current.ready = true;
              void current.engine
                .ready()
                .then(() => {
                  if (!closed && active === current && current.child.exitCode === null)
                    emit({ type: "ready", urls: message.urls, capabilities: message.capabilities });
                })
                .catch(async (error) => {
                  if (!closed && active === current) {
                    report(error);
                    await stopActive();
                  }
                });
            },
          });
          const launched = { child, engine, ready: false };
          active = launched;
          void child.exited.then(async (code) => {
            if (active !== launched || closed) return;
            emit({ type: "exited", code });
            await stopActive();
          });
        } catch (error) {
          try {
            await engine.close();
          } catch (cleanup) {
            cleanupFailed(cleanup);
          }
          report(error);
        }
      }
    })().finally(() => {
      restarting = undefined;
    });
    return restarting;
  }
  function close(): Promise<void> {
    return (closing ??= (async () => {
      closed = true;
      if (debounce) clearTimeout(debounce);
      for (const watcher of watchers.values()) watcher.close();
      watchers.clear();
      try {
        await restarting;
        await stopActive();
        if (cleanupFailures.length === 1) throw cleanupFailures[0];
        if (cleanupFailures.length > 1)
          throw new AggregateError(cleanupFailures, "Development cleanup failed.");
      } finally {
        finish();
      }
    })());
  }
  try {
    await restart();
  } catch (error) {
    try {
      await close();
    } catch (cleanup) {
      throw new AggregateError([error, cleanup], "Development startup and cleanup failed.");
    }
    throw error;
  }
  return Object.freeze({ close, done });
}
