import { realpathSync } from "node:fs";
import { resolve } from "node:path";
import { isDevReadyMessage } from "./dev-ready";
import { devConditionArgs, startObservedEngineDevCycle, type EngineDevCycle } from "./engine-dev";
import { diagnostic, EngineError, type EngineDiagnostic } from "./diagnostics";
import type { ApplicationTarget } from "./application";
import { checkedEntry } from "./engine-host";
import { SourceInputs } from "./source-inputs";
import { InputWatches } from "./input-watches";

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
  const watchers = new InputWatches(root, changed, report);
  let watchFiles: readonly string[] = [];
  let sourceDirectories: readonly string[] = [];
  let chosenEntry = resolve(root, options.entry ?? "src/server.ts");

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
  async function recoveryWatches() {
    const inputs = new SourceInputs(root);
    await inputs.configuration(root);
    for (const path of ["lenso.engine.ts", options.config ?? "lenso.config.ts", chosenEntry])
      await inputs.add(resolve(root, path));
    if (!closed)
      watchers.replace(
        [...watchFiles, ...inputs.files],
        [...sourceDirectories, ...inputs.directories],
      );
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
        if (!watchFiles.length) {
          const inputs = new SourceInputs(root);
          await inputs.configuration(root);
          watchers.replace([
            ...inputs.files,
            resolve(root, "lenso.engine.ts"),
            resolve(root, options.config ?? "lenso.config.ts"),
            chosenEntry,
          ]);
        }
        let engine: EngineDevCycle;
        try {
          engine = await startObservedEngineDevCycle(
            { root, config: options.config },
            options.entry,
            (paths, directories, replace) => {
              watchFiles = replace ? paths : [...new Set([...watchFiles, ...paths])];
              sourceDirectories = replace
                ? directories
                : [...new Set([...sourceDirectories, ...directories])];
              if (!closed) {
                try {
                  watchers.replace(watchFiles, sourceDirectories);
                } catch (error) {
                  report(error);
                }
              }
            },
          );
        } catch (error) {
          if (includesCleanup(diagnostic(error))) cleanupFailures.push(error);
          report(error);
          await recoveryWatches();
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
          chosenEntry = engine.entry;
          watchFiles = [...engine.watchFiles, resolve(root, "lenso.engine.ts")];
          watchers.replace(watchFiles, sourceDirectories);
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
      watchers.close();
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
