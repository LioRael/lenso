import {
  realpathSync,
  readFileSync,
  writeFileSync,
  unlinkSync,
  existsSync,
  mkdtempSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
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
  | { type: "ready"; pid?: number; urls?: readonly string[]; capabilities?: readonly string[] }
  | { type: "failed"; diagnostic: EngineDiagnostic }
  | { type: "exited"; code: number };

export interface DevSupervisorOptions extends ApplicationTarget {
  entry?: string;
  /** Opt-in private descriptor published only after ready hooks succeed. */
  connectionFile?: string;
  /** Observers cannot interrupt resource ownership; synchronous exceptions are ignored. */
  onEvent?(event: DevSupervisorEvent): void;
  stdout?: "inherit" | "ignore";
  stderr?: "inherit" | "ignore";
  env?: Record<string, string | undefined> | (() => Record<string, string | undefined>);
  signal?: AbortSignal;
  stopTimeoutMs?: number;
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

async function stopProcess(previous: ReturnType<typeof Bun.spawn>, timeoutMs = 5000) {
  if (previous.exitCode !== null) return;
  let forced = false;
  previous.kill("SIGTERM");
  const timeout = setTimeout(() => {
    forced = true;
    previous.kill("SIGKILL");
  }, timeoutMs);
  try {
    await previous.exited;
    if (forced)
      throw new EngineError({
        code: "dev-runtime-timeout",
        phase: "engine-cleanup",
        message: `Development runtime shutdown exceeded ${timeoutMs / 1000} seconds; the process was terminated.`,
      });
  } finally {
    clearTimeout(timeout);
  }
}

export async function createDevSupervisor(options: DevSupervisorOptions): Promise<DevSupervisor> {
  if (options.connectionFile && process.platform === "win32")
    throw new EngineError({
      code: "unsupported",
      phase: "engine-dev",
      message: "Development operation connections currently require a Unix host.",
    });
  if (
    options.stopTimeoutMs !== undefined &&
    (!Number.isFinite(options.stopTimeoutMs) || options.stopTimeoutMs <= 0)
  )
    throw new EngineError({
      code: "invalid-engine-state",
      phase: "engine-dev",
      message: "Development shutdown timeout must be a positive finite number.",
    });
  const root = realpathSync(resolve(options.root));
  type Active = {
    child: ReturnType<typeof Bun.spawn>;
    engine: EngineDevCycle;
    ready: boolean;
    generation: string;
    connectionDirectory?: string;
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
    if (previous) {
      try {
        previous.child.send({ type: "lenso:dev-revoke", generation: previous.generation });
      } catch {
        /* Process may already be gone. */
      }
      if (options.connectionFile) {
        const file = resolve(root, options.connectionFile);
        try {
          if (JSON.parse(readFileSync(file, "utf8")).generation === previous.generation)
            unlinkSync(file);
        } catch {
          /* Never remove an unowned descriptor. */
        }
      }
    }
    if (previous)
      stopping = stopping.then(async () => {
        try {
          await stopProcess(previous.child, options.stopTimeoutMs);
        } catch (error) {
          cleanupFailed(error);
        }
        if (previous.connectionDirectory) {
          try {
            rmSync(previous.connectionDirectory, { recursive: true, force: true });
          } catch (error) {
            cleanupFailed(error);
          }
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
        const env = typeof options.env === "function" ? options.env() : options.env;
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
            env,
            options.signal,
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
        let connectionDirectory: string | undefined;
        try {
          chosenEntry = engine.entry;
          watchFiles = [...engine.watchFiles, resolve(root, "lenso.engine.ts")];
          watchers.replace(watchFiles, sourceDirectories);
          const entry = await checkedEntry(root, engine.entry, "dev");
          const generation = crypto.randomUUID();
          const codeHash = options.connectionFile ? createHash("sha256") : undefined;
          for (const file of options.connectionFile ? [...new Set(watchFiles)].sort() : []) {
            try {
              codeHash?.update(file).update(readFileSync(file));
            } catch {
              /* Missing watches are explicit limitations. */
            }
          }
          const provenance = {
            generation,
            codeRevision: {
              kind: "watched-inputs-sha256",
              digest: codeHash?.digest("hex") ?? null,
              scope:
                "Watched inputs sampled before launch; not loaded-byte attestation or a release SHA",
              verifiedLoadedBytes: false,
            },
          };
          if (options.connectionFile && existsSync(resolve(root, options.connectionFile)))
            throw new EngineError({
              code: "dev-connection-in-use",
              phase: "engine-dev",
              message: "Connection descriptor already exists; select another owned path.",
            });
          connectionDirectory = options.connectionFile
            ? mkdtempSync(join(tmpdir(), "lenso-dev-"))
            : undefined;
          const child = Bun.spawn([process.execPath, ...devConditionArgs(), entry], {
            cwd: root,
            env: {
              ...process.env,
              ...env,
              LENSO_DEV_PROVENANCE: options.connectionFile ? JSON.stringify(provenance) : undefined,
              LENSO_DEV_CONFIG: options.connectionFile
                ? resolve(root, options.config ?? "lenso.config.ts")
                : undefined,
              LENSO_DEV_GENERATION: options.connectionFile ? generation : undefined,
              LENSO_DEV_DESCRIPTOR: options.connectionFile
                ? resolve(root, options.connectionFile)
                : undefined,
              LENSO_DEV_SOCKET: connectionDirectory
                ? join(connectionDirectory, "operations.sock")
                : undefined,
            },
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
                  if (!closed && active === current && current.child.exitCode === null) {
                    if (options.connectionFile && !message.connection)
                      throw new EngineError({
                        code: "dev-connection-unsupported",
                        phase: "engine-dev",
                        message: "Entry did not attach an explicit borrowed operation host.",
                      });
                    if (options.connectionFile && message.connection) {
                      if (message.connection.generation !== generation)
                        throw new EngineError({
                          code: "stale-generation",
                          phase: "engine-dev",
                          message: "Runtime reported a different generation.",
                        });
                      writeFileSync(
                        resolve(root, options.connectionFile),
                        JSON.stringify({
                          schemaVersion: 1,
                          root,
                          ...message.connection,
                          provenance,
                        }),
                        { flag: "wx", mode: 0o600 },
                      );
                      current.child.send({ type: "lenso:dev-admit", generation });
                    }
                    emit({
                      type: "ready",
                      pid: current.child.pid,
                      urls: message.urls,
                      capabilities: message.capabilities,
                    });
                  }
                })
                .catch(async (error) => {
                  if (!closed && active === current) {
                    report(error);
                    await stopActive();
                  }
                });
            },
          });
          const launched = { child, engine, ready: false, generation, connectionDirectory };
          active = launched;
          void child.exited.then(async (code) => {
            if (active !== launched || closed) return;
            emit({ type: "exited", code });
            await stopActive();
          });
        } catch (error) {
          if (connectionDirectory) rmSync(connectionDirectory, { recursive: true, force: true });
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
      options.signal?.removeEventListener("abort", aborted);
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
  function aborted() {
    void close().catch(report);
  }
  options.signal?.addEventListener("abort", aborted, { once: true });
  if (options.signal?.aborted) aborted();
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
