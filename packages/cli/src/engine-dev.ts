import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { CliError, type CliDiagnostic } from "./diagnostics";

export interface EngineDevCycle {
  readonly watchFiles: readonly string[];
  ready(): Promise<void>;
  close(): Promise<void>;
}
/** Each cycle imports a fresh config/dependency graph and owns its build resources. */
export async function startEngineDevCycle(root: string): Promise<EngineDevCycle> {
  const extension = import.meta.url.endsWith(".ts") ? "ts" : "js";
  const workerPath = fileURLToPath(new URL(`./engine-dev-worker.${extension}`, import.meta.url));
  let resolvePrepared!: (paths: readonly string[]) => void;
  let rejectPrepared!: (error: unknown) => void;
  const prepared = new Promise<readonly string[]>((resolve, reject) => {
    resolvePrepared = resolve;
    rejectPrepared = reject;
  });
  let failure: CliError | undefined;
  let stopped = false;
  let closing: Promise<void> | undefined;
  let sequence = 0;
  const pending = new Map<number, { resolve(): void; reject(cause: unknown): void }>();
  const failed = (error: CliError) => {
    failure ??= error;
    rejectPrepared(failure);
    for (const request of pending.values()) request.reject(failure);
    pending.clear();
  };
  const child = Bun.spawn([process.execPath, workerPath, resolve(root)], {
    cwd: resolve(root),
    stdout: "ignore",
    stderr: "inherit",
    ipc(message: unknown) {
      if (!message || typeof message !== "object" || !("type" in message)) return;
      if (
        message.type === "prepared" &&
        "watchFiles" in message &&
        Array.isArray(message.watchFiles) &&
        message.watchFiles.every((path) => typeof path === "string")
      )
        resolvePrepared(message.watchFiles);
      if (message.type === "closed") stopped = true;
      if (message.type === "failed" && "error" in message)
        failed(new CliError(message.error as CliDiagnostic));
      if (message.type === "ack" && "id" in message && typeof message.id === "number") {
        pending.get(message.id)?.resolve();
        pending.delete(message.id);
      }
    },
  });
  void child.exited.then((code) => {
    if (!stopped || code !== 0)
      failed(
        new CliError({
          code: "engine-worker-exited",
          phase: "engine-dev",
          message: "Engine development worker exited before completing cleanup.",
        }),
      );
  });
  const close = () =>
    (closing ??= (async () => {
      const timeout = setTimeout(() => child.kill("SIGKILL"), 5000);
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
      new CliError({
        code: "engine-worker-timeout",
        phase: "engine-dev",
        message: "Engine development startup exceeded 30 seconds.",
      }),
    );
  }, 30000);
  try {
    const watchFiles = await prepared;
    return Object.freeze({
      watchFiles: Object.freeze([...watchFiles]),
      ready() {
        if (failure) return Promise.reject(failure);
        if (closing || child.exitCode !== null)
          return Promise.reject(
            new CliError({
              code: "engine-worker-closed",
              phase: "engine-dev",
              message: "Engine development cycle is closed.",
            }),
          );
        const id = ++sequence;
        return new Promise<void>((resolve, reject) => {
          pending.set(id, { resolve, reject });
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
    } catch {
      /* Preserve the attributed startup error. */
    }
    throw cause;
  } finally {
    clearTimeout(startupTimeout);
  }
}
