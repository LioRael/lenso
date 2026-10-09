import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { EngineError, type EngineDiagnostic } from "./diagnostics";
import { resolveApplicationTarget, type ApplicationTarget } from "./application";
import { conditionArgs } from "./resolution";

export interface EngineDevCycle {
  readonly watchFiles: readonly string[];
  readonly entry: string;
  ready(): Promise<void>;
  close(): Promise<void>;
}

export function devConditionArgs(args: readonly string[] = process.execArgv): string[] {
  return conditionArgs(args);
}

/** Each cycle imports a fresh config/dependency graph and owns its build resources. */
export async function startEngineDevCycle(
  target: string | ApplicationTarget,
  entry?: string,
): Promise<EngineDevCycle> {
  return startObservedEngineDevCycle(target, entry);
}

/** Supervisor-only invalidation updates, including inputs acquired before a failed hook. */
export async function startObservedEngineDevCycle(
  target: string | ApplicationTarget,
  entry?: string,
  observe?: (paths: readonly string[], directories: readonly string[], replace: boolean) => void,
): Promise<EngineDevCycle> {
  const { root, config } = resolveApplicationTarget(target);
  const extension = import.meta.url.endsWith(".ts") ? "ts" : "js";
  // Bundled embedders relocate import.meta.url; the owned worker stays in its installed package.
  const workerPath =
    extension === "ts"
      ? fileURLToPath(new URL("./engine-dev-worker.ts", import.meta.url))
      : Bun.resolveSync("@lenso/engine/dev-worker", import.meta.dir);
  let resolvePrepared!: (value: { watchFiles: readonly string[]; entry: string }) => void;
  let rejectPrepared!: (error: unknown) => void;
  const prepared = new Promise<{ watchFiles: readonly string[]; entry: string }>(
    (complete, reject) => {
      resolvePrepared = complete;
      rejectPrepared = reject;
    },
  );
  let failure: EngineError | undefined;
  let currentWatchFiles: readonly string[] = [];
  let stopped = false;
  let closing: Promise<void> | undefined;
  let sequence = 0;
  const pending = new Map<number, { resolve(): void; reject(cause: unknown): void }>();
  const failed = (error: EngineError) => {
    failure = error;
    rejectPrepared(error);
    for (const request of pending.values()) request.reject(error);
    pending.clear();
  };
  const child = Bun.spawn(
    [process.execPath, ...devConditionArgs(), workerPath, root, JSON.stringify({ config, entry })],
    {
      cwd: resolve(root),
      stdout: "ignore",
      stderr: "inherit",
      ipc(message: unknown) {
        if (!message || typeof message !== "object" || !("type" in message)) return;
        if (
          message.type === "watch-inputs" &&
          "watchFiles" in message &&
          Array.isArray(message.watchFiles) &&
          message.watchFiles.every((path) => typeof path === "string") &&
          "sourceDirectories" in message &&
          Array.isArray(message.sourceDirectories) &&
          message.sourceDirectories.every((path) => typeof path === "string")
        ) {
          currentWatchFiles = Object.freeze([...message.watchFiles]);
          observe?.(
            currentWatchFiles,
            message.sourceDirectories,
            "replace" in message && message.replace === true,
          );
        }
        if (
          message.type === "prepared" &&
          "watchFiles" in message &&
          Array.isArray(message.watchFiles) &&
          message.watchFiles.every((path) => typeof path === "string") &&
          "entry" in message &&
          typeof message.entry === "string" &&
          "sourceDirectories" in message &&
          Array.isArray(message.sourceDirectories) &&
          message.sourceDirectories.every((path) => typeof path === "string")
        ) {
          currentWatchFiles = Object.freeze([...message.watchFiles]);
          observe?.(currentWatchFiles, message.sourceDirectories, true);
          resolvePrepared({ watchFiles: message.watchFiles, entry: message.entry });
        }
        if (message.type === "closed") stopped = true;
        if (message.type === "failed" && "error" in message) {
          failed(new EngineError(message.error as EngineDiagnostic));
        }
        if (message.type === "ack" && "id" in message && typeof message.id === "number") {
          pending.get(message.id)?.resolve();
          pending.delete(message.id);
        }
      },
    },
  );
  void child.exited.then((code) => {
    if ((!stopped || code !== 0) && !failure)
      failed(
        new EngineError({
          code: "engine-worker-exited",
          phase: "engine-dev",
          message: "Engine development worker exited before completing cleanup.",
        }),
      );
  });
  const close = () =>
    (closing ??= (async () => {
      const timeout = setTimeout(() => {
        failed(
          new EngineError({
            code: "engine-worker-timeout",
            phase: "engine-cleanup",
            message: "Engine development cleanup exceeded 5 seconds; the worker was terminated.",
          }),
        );
        child.kill("SIGKILL");
      }, 5000);
      try {
        if (child.exitCode === null) {
          try {
            child.send({ type: "close" });
          } catch {
            child.kill("SIGTERM");
          }
        }
        await child.exited;
        if (failure) throw failure;
      } finally {
        clearTimeout(timeout);
      }
    })());
  const startupTimeout = setTimeout(() => {
    failed(
      new EngineError({
        code: "engine-worker-timeout",
        phase: "engine-dev",
        message: "Engine development startup exceeded 30 seconds.",
      }),
    );
  }, 30000);
  try {
    const { entry: chosenEntry } = await prepared;
    return Object.freeze({
      get watchFiles() {
        return currentWatchFiles;
      },
      entry: chosenEntry,
      ready() {
        if (failure) return Promise.reject(failure);
        if (closing || child.exitCode !== null)
          return Promise.reject(
            new EngineError({
              code: "engine-worker-closed",
              phase: "engine-dev",
              message: "Engine development cycle is closed.",
            }),
          );
        const id = ++sequence;
        return new Promise<void>((complete, reject) => {
          pending.set(id, { resolve: complete, reject });
          try {
            child.send({ type: "ready", id });
          } catch (cause) {
            pending.delete(id);
            reject(cause);
          }
        });
      },
      close,
    });
  } catch (cause) {
    try {
      await close();
    } catch (cleanup) {
      if (cleanup !== cause)
        throw new AggregateError([cause, cleanup], "Engine startup and cleanup failed.");
    }
    throw cause;
  } finally {
    clearTimeout(startupTimeout);
  }
}
