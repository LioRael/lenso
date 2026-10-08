import { watch, realpathSync, statSync, type FSWatcher } from "node:fs";
import { resolve, relative, extname, sep } from "node:path";
import { createDevPresentation, type DevPresentation, type DevReady } from "./dev-presentation";
import { startEngineDevCycle, type EngineDevCycle } from "./engine-dev";
import { diagnostic } from "./diagnostics";

interface DevOptions {
  root: string;
  entry?: string;
  presentation?: DevPresentation;
}

/** The entry sends this over Bun IPC after its app and listeners have started. */
export interface DevReadyMessage extends DevReady {
  readonly type: "lenso:dev-ready";
  readonly urls?: readonly string[];
}

function isDevReadyMessage(message: unknown): message is DevReadyMessage {
  if (!message || typeof message !== "object" || !("type" in message)) return false;
  if (message.type !== "lenso:dev-ready") return false;
  for (const key of ["urls", "capabilities"] as const) {
    if (key in message) {
      const values: unknown = Reflect.get(message, key);
      if (!Array.isArray(values) || !values.every((value) => typeof value === "string"))
        return false;
    }
  }
  return true;
}

/** Stops the previous runtime and build resources before starting a fresh module graph. */
export async function dev(options: DevOptions): Promise<void> {
  const root = realpathSync(resolve(options.root));
  const entry = resolve(root, options.entry ?? "src/server.ts");
  if (!(await Bun.file(entry).exists())) throw new Error(`Development entry missing: ${entry}`);
  const presentation = options.presentation ?? createDevPresentation({ project: root });
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
  const done = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const watchers = new Map<string, FSWatcher>();
  const ignored = new Set([".lenso", "dist", "node_modules", ".git", ".turbo", ".wrangler"]);

  function report(error: unknown) {
    console.error("[lenso]", diagnostic(error));
    presentation.failed();
  }
  async function stopProcess(previous: ReturnType<typeof Bun.spawn>) {
    if (previous.exitCode !== null) return;
    previous.kill("SIGTERM");
    const timeout = setTimeout(() => previous.kill("SIGKILL"), 5000);
    try {
      await previous.exited;
    } finally {
      clearTimeout(timeout);
    }
  }
  function stopActive(): Promise<void> {
    const previous = active;
    active = undefined;
    if (previous)
      stopping = stopping
        .then(async () => {
          try {
            await stopProcess(previous.child);
          } finally {
            await previous.engine.close();
          }
        })
        .catch(report);
    return stopping;
  }
  function changed() {
    if (closed) return;
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(() => {
      void restart().catch(report);
    }, 100);
  }
  function watchPath(path: string, sourceOnly = false) {
    const canonical = realpathSync(path);
    if (watchers.has(canonical)) return;
    const directory = statSync(canonical).isDirectory();
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
      if (
        sourceOnly &&
        ![".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".json"].includes(extname(changedPath))
      )
        return;
      changed();
    });
    watcher.on("error", report);
    watchers.set(canonical, watcher);
  }
  function resetWatches(paths: readonly string[] = []) {
    for (const watcher of watchers.values()) watcher.close();
    watchers.clear();
    if (closed) return;
    // This fallback also observes newly added configs/imports after a failed build.
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
        presentation.starting();
        resetWatches();
        let engine: EngineDevCycle;
        try {
          engine = await startEngineDevCycle(root);
        } catch (error) {
          report(error);
          continue;
        }
        if (closed) {
          await engine.close();
          break;
        }
        try {
          resetWatches(engine.watchFiles);
          const child = Bun.spawn([process.execPath, entry], {
            cwd: root,
            stdout: "inherit",
            stderr: "inherit",
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
                  if (!closed && active === current) presentation.ready(message);
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
            console.error(`[lenso] Development process exited (${code}). Edit source to restart.`);
            presentation.failed();
            await stopActive();
          });
        } catch (error) {
          try {
            await engine.close();
          } catch (cleanup) {
            report(cleanup);
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
      } finally {
        finish();
      }
    })());
  }
  const onSignal = () => {
    void close().catch(report);
  };
  process.on("SIGINT", onSignal);
  process.on("SIGTERM", onSignal);
  try {
    await restart();
    await done;
  } finally {
    process.off("SIGINT", onSignal);
    process.off("SIGTERM", onSignal);
    await close();
  }
}
