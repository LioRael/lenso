import { resolve } from "node:path";
import { createDevSupervisor, type DevSupervisor } from "./dev";
import type { ApplicationTarget } from "./application";
import { InputWatches } from "./input-watches";
import { EngineError } from "./diagnostics";

function workspaceError(code: string, message: string): EngineError {
  return new EngineError({ code, phase: "workspace-dev", message });
}

export interface WorkspaceProcessContext {
  instance: string;
  process: ReturnType<typeof Bun.spawn>;
  signal: AbortSignal;
}
interface WorkspaceServiceBase {
  id: string;
  dependsOn?: readonly string[];
  env?: Record<string, string>;
  watch?: readonly string[];
  startupTimeoutMs?: number;
  stopTimeoutMs?: number;
}
export interface WorkspaceApplicationService extends WorkspaceServiceBase {
  kind: "application";
  target: ApplicationTarget;
  entry?: string;
}
export interface WorkspaceProcessService extends WorkspaceServiceBase {
  kind: "process";
  cwd: string;
  command: readonly string[];
  build?: readonly string[];
  ready(context: WorkspaceProcessContext): Promise<void | { urls?: readonly string[] }>;
  activate?(context: WorkspaceProcessContext): Promise<void>;
}
export type WorkspaceService = WorkspaceApplicationService | WorkspaceProcessService;
export interface WorkspaceDefinition {
  services: readonly WorkspaceService[];
  prepare?(context: { signal: AbortSignal }): Promise<void>;
}
export interface WorkspaceServiceState {
  id: string;
  status: "starting" | "building" | "ready" | "failed" | "stopped";
  instance?: string;
  version: number;
  pid?: number;
  elapsedMs: number;
  urls?: readonly string[];
  error?: unknown;
  retained?: { instance: string; version: number; pid?: number };
}
export interface WorkspaceSupervisorOptions {
  workspace: WorkspaceDefinition;
  root?: string;
  signal?: AbortSignal;
  stdout?: "inherit" | "ignore";
  stderr?: "inherit" | "ignore";
  onEvent?(event: WorkspaceServiceState): void;
}
export interface WorkspaceSupervisor {
  readonly done: Promise<void>;
  readonly states: readonly WorkspaceServiceState[];
  rebuild(id: string): Promise<void>;
  close(): Promise<void>;
}

function ordered(workspace: WorkspaceDefinition): WorkspaceService[] {
  const services = new Map<string, WorkspaceService>();
  for (const service of workspace.services) {
    if (typeof service.id !== "string" || !service.id.trim() || services.has(service.id))
      throw workspaceError("invalid-workspace", `Invalid or duplicate workspace ID: ${service.id}`);
    for (const bound of [service.startupTimeoutMs, service.stopTimeoutMs])
      if (bound !== undefined && (!Number.isFinite(bound) || bound <= 0))
        throw workspaceError("invalid-workspace", `Invalid workspace timeout: ${service.id}`);
    if (service.kind === "process" && (!service.command.length || service.build?.length === 0))
      throw workspaceError("invalid-workspace", `Empty workspace command: ${service.id}`);
    services.set(service.id, service);
  }
  const visiting = new Set<string>(),
    visited = new Set<string>(),
    result: WorkspaceService[] = [];
  function visit(id: string) {
    if (visited.has(id)) return;
    if (visiting.has(id))
      throw workspaceError("invalid-workspace", `Cyclic workspace dependency: ${id}`);
    const service = services.get(id);
    if (!service) throw workspaceError("invalid-workspace", `Missing workspace dependency: ${id}`);
    visiting.add(id);
    for (const dependency of service.dependsOn ?? []) visit(dependency);
    visiting.delete(id);
    visited.add(id);
    result.push(service);
  }
  for (const id of services.keys()) visit(id);
  return result;
}

export function defineWorkspace<T extends WorkspaceDefinition>(workspace: T): T {
  ordered(workspace);
  return workspace;
}

export async function createWorkspaceSupervisor(
  options: WorkspaceSupervisorOptions,
): Promise<WorkspaceSupervisor> {
  const services = ordered(options.workspace);
  const root = resolve(options.root ?? process.cwd());
  const controller = new AbortController();
  const signal = controller.signal;
  type Active = {
    instance: string;
    version: number;
    pid?: number;
    startupTimer?: ReturnType<typeof setTimeout>;
    controller?: AbortController;
    process?: ReturnType<typeof Bun.spawn>;
    dev?: DevSupervisor;
  };
  const active = new Map<string, Active>();
  const owned = new Map<ReturnType<typeof Bun.spawn>, number>();
  const stops = new Map<ReturnType<typeof Bun.spawn>, Promise<void>>();
  const devs = new Set<DevSupervisor>();
  const controllers = new Set<AbortController>();
  const states = new Map<string, WorkspaceServiceState>();
  const jobs = new Map<string, Promise<void>>();
  const queued = new Set<string>();
  const watches: InputWatches[] = [];
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  const versions = new Map<string, number>();
  let closed = false,
    closing: Promise<void> | undefined;
  let failure: unknown;
  let finish!: (error?: unknown) => void;
  const done = new Promise<void>((complete, reject) => {
    finish = (error) => (error === undefined ? complete() : reject(error));
  });
  void done.catch(() => {});
  function emit(state: WorkspaceServiceState) {
    states.set(state.id, Object.freeze(state));
    try {
      options.onEvent?.(state);
    } catch {
      /* Observers do not own resources. */
    }
  }
  async function bounded<T>(work: Promise<T>, ms: number, operationSignal = signal): Promise<T> {
    void work.catch(() => {});
    operationSignal.throwIfAborted();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let abort!: () => void;
    try {
      return await Promise.race([
        work,
        new Promise<never>((_, reject) => {
          abort = () => reject(operationSignal.reason);
          operationSignal.addEventListener("abort", abort, { once: true });
          timer = setTimeout(
            () =>
              reject(
                workspaceError("workspace-startup-timeout", `Workspace startup exceeded ${ms}ms`),
              ),
            ms,
          );
        }),
      ]);
    } finally {
      clearTimeout(timer);
      operationSignal.removeEventListener("abort", abort);
    }
  }
  function stop(child: ReturnType<typeof Bun.spawn>): Promise<void> {
    const previous = stops.get(child);
    if (previous) return previous;
    const stopping = stopOwned(child).finally(() => stops.delete(child));
    stops.set(child, stopping);
    return stopping;
  }
  async function stopOwned(child: ReturnType<typeof Bun.spawn>) {
    if (child.exitCode !== null) {
      owned.delete(child);
      return;
    }
    child.kill("SIGTERM");
    let forced = false;
    const timeoutMs = owned.get(child) ?? 5000;
    const timer = setTimeout(() => {
      forced = true;
      child.kill("SIGKILL");
    }, timeoutMs);
    try {
      await child.exited;
      if (forced)
        throw workspaceError(
          "workspace-stop-timeout",
          `Workspace process ${child.pid} shutdown exceeded ${timeoutMs}ms; it was terminated.`,
        );
    } finally {
      clearTimeout(timer);
      owned.delete(child);
    }
  }
  async function stopActive(value: Active) {
    clearTimeout(value.startupTimer);
    value.controller?.abort(new Error("Workspace instance stopped"));
    if (value.controller) controllers.delete(value.controller);
    if (value.process) await stop(value.process);
    if (value.dev) {
      try {
        await value.dev.close();
      } finally {
        devs.delete(value.dev);
      }
    }
  }
  function fatal(service: WorkspaceService, error: unknown) {
    if (closed) return;
    failure = error;
    emit({ ...states.get(service.id)!, id: service.id, status: "failed", error });
    void close().catch(() => {});
  }
  function spawn(service: WorkspaceProcessService, command: readonly string[], instance: string) {
    const child = Bun.spawn([...command], {
      cwd: resolve(root, service.cwd),
      env: { ...process.env, ...service.env, LENSO_WORKSPACE_INSTANCE: instance },
      stdout: options.stdout ?? "inherit",
      stderr: options.stderr ?? "inherit",
    });
    owned.set(child, service.stopTimeoutMs ?? 5000);
    return child;
  }
  async function cycle(service: WorkspaceService, initial: boolean) {
    let started = performance.now();
    let previous = active.get(service.id);
    let instance = crypto.randomUUID();
    let version = (versions.get(service.id) ?? 0) + 1;
    versions.set(service.id, version);
    const candidate: Active = { instance, version };
    const candidateController = new AbortController();
    candidate.controller = candidateController;
    let activating = false;
    controllers.add(candidateController);
    const abortCandidate = () => candidateController.abort(signal.reason);
    signal.addEventListener("abort", abortCandidate, { once: true });
    const startupTimer = setTimeout(
      () =>
        candidateController.abort(
          workspaceError(
            "workspace-startup-timeout",
            `Workspace startup exceeded ${service.startupTimeoutMs ?? 30000}ms`,
          ),
        ),
      service.startupTimeoutMs ?? 30000,
    );
    const state = (
      status: WorkspaceServiceState["status"],
      extra: Partial<WorkspaceServiceState> = {},
    ) =>
      emit({
        id: service.id,
        status,
        instance,
        version,
        pid: candidate.process?.pid ?? candidate.pid,
        elapsedMs: performance.now() - started,
        ...extra,
      });
    try {
      signal.throwIfAborted();
      const timeout = service.startupTimeoutMs ?? 30000;
      if (service.kind === "process") {
        if (service.build) {
          const build = spawn(service, service.build, instance);
          state("building", { pid: build.pid });
          try {
            const code = await bounded(build.exited, timeout, candidateController.signal);
            if (code !== 0)
              throw workspaceError(
                "workspace-build-failed",
                `Workspace build failed (${code}): ${service.id}`,
              );
          } finally {
            await stop(build);
          }
        }
        signal.throwIfAborted();
        candidate.process = spawn(service, service.command, instance);
        state("starting");
        const context = {
          instance,
          process: candidate.process,
          signal: candidateController.signal,
        };
        const exited = candidate.process.exited.then((code) => {
          throw workspaceError(
            "workspace-process-exited",
            `Workspace process exited (${code}): ${service.id}`,
          );
        });
        const ready = await bounded(
          Promise.race([
            Promise.resolve().then(() => {
              context.signal.throwIfAborted();
              return service.ready(context);
            }),
            exited,
          ]),
          timeout,
          candidateController.signal,
        );
        candidateController.signal.throwIfAborted();
        if (service.activate) {
          activating = true;
          await bounded(
            Promise.race([
              Promise.resolve().then(() => {
                context.signal.throwIfAborted();
                return service.activate!(context);
              }),
              exited,
            ]),
            timeout,
            candidateController.signal,
          );
        }
        candidateController.signal.throwIfAborted();
        signal.throwIfAborted();
        if (candidate.process.exitCode !== null)
          throw workspaceError(
            "workspace-process-exited",
            `Workspace process exited: ${service.id}`,
          );
        active.set(service.id, candidate);
        activating = false;
        state("ready", { urls: ready?.urls });
        const launched = candidate;
        void candidate.process.exited.then((code) => {
          if (active.get(service.id) === launched)
            fatal(
              service,
              workspaceError(
                "workspace-process-exited",
                `Workspace process exited (${code}): ${service.id}`,
              ),
            );
        });
      } else {
        if (previous) {
          active.delete(service.id);
          await stopActive(previous);
          previous = undefined;
        }
        state("starting");
        let ready!: (urls?: readonly string[]) => void, failed!: (error: unknown) => void;
        const readiness = new Promise<readonly string[] | undefined>((complete, reject) => {
          ready = complete;
          failed = reject;
        });
        // Attach before launching, since Engine can report failure during creation.
        void readiness.catch(() => {});
        candidate.dev = await createDevSupervisor({
          ...service.target,
          root: resolve(root, service.target.root),
          entry: service.entry,
          env: () => {
            if (candidate.dev) {
              started = performance.now();
              candidate.pid = undefined;
              instance = crypto.randomUUID();
              version = (versions.get(service.id) ?? 0) + 1;
              versions.set(service.id, version);
              candidate.instance = instance;
              candidate.version = version;
            }
            return { ...service.env, LENSO_WORKSPACE_INSTANCE: instance };
          },
          signal: candidateController.signal,
          stopTimeoutMs: service.stopTimeoutMs,
          stdout: options.stdout,
          stderr: options.stderr,
          onEvent(event) {
            if (event.type === "ready") {
              clearTimeout(candidate.startupTimer);
              candidate.pid = event.pid;
              ready(event.urls);
              if (active.get(service.id)?.instance === instance)
                state("ready", { urls: event.urls, pid: event.pid });
            } else if (event.type === "failed" || event.type === "exited") {
              const error =
                event.type === "failed"
                  ? new EngineError(event.diagnostic)
                  : workspaceError(
                      "workspace-process-exited",
                      `Workspace application exited (${event.code})`,
                    );
              failed(error);
              if (active.get(service.id)?.instance === instance) fatal(service, error);
            } else if (active.get(service.id)?.instance === instance) {
              clearTimeout(candidate.startupTimer);
              candidate.startupTimer = setTimeout(
                () =>
                  fatal(
                    service,
                    workspaceError(
                      "workspace-startup-timeout",
                      `Workspace application startup exceeded ${timeout}ms: ${service.id}`,
                    ),
                  ),
                timeout,
              );
              state("starting");
            }
          },
        });
        devs.add(candidate.dev);
        candidateController.signal.throwIfAborted();
        const urls = await bounded(readiness, timeout, candidateController.signal);
        candidateController.signal.throwIfAborted();
        active.set(service.id, candidate);
        state("ready", { urls });
      }
      clearTimeout(startupTimer);
      if (previous) await stopActive(previous);
    } catch (error) {
      candidateController.abort(error);
      let candidateError = error;
      const uncertain =
        error instanceof EngineError &&
        (error.diagnostic.code === "workspace-cutover-unknown" ||
          (activating &&
            ["workspace-startup-timeout", "workspace-process-exited"].includes(
              error.diagnostic.code,
            )));
      if (uncertain) {
        candidateError = new EngineError(
          {
            code: "workspace-cutover-unknown",
            phase: "workspace-dev",
            message: `Workspace cutover outcome is unknown: ${service.id}; all owned instances are being stopped.`,
          },
          { cause: error },
        );
        fatal(service, candidateError);
      }
      if (active.get(service.id) === candidate) {
        fatal(service, error);
        throw error;
      }
      try {
        await stopActive(candidate);
      } catch (cleanup) {
        candidateError = new AggregateError(
          [error, cleanup],
          "Workspace candidate and cleanup failed",
          { cause: error },
        );
      }
      controllers.delete(candidateController);
      state("failed", {
        error: candidateError,
        retained:
          previous && !closed
            ? {
                instance: previous.instance,
                version: previous.version,
                pid: previous.process?.pid ?? previous.pid,
              }
            : undefined,
      });
      if (initial || uncertain) throw candidateError;
    } finally {
      clearTimeout(startupTimer);
      signal.removeEventListener("abort", abortCandidate);
    }
  }
  function rebuild(id: string): Promise<void> {
    const service = services.find((item) => item.id === id);
    if (!service) return Promise.reject(new Error(`Unknown workspace service: ${id}`));
    if (closed) return Promise.reject(new Error("Workspace is closed"));
    queued.add(id);
    const existing = jobs.get(id);
    if (existing) return existing;
    const job = Promise.resolve()
      .then(async () => {
        while (queued.delete(id) && !closed) await cycle(service, false);
      })
      .finally(() => {
        jobs.delete(id);
      });
    jobs.set(id, job);
    return job;
  }
  function close(): Promise<void> {
    if (closing) return closing;
    closed = true;
    return (closing = Promise.resolve().then(async () => {
      const watchFailures: unknown[] = [];
      controller.abort(new Error("Workspace closed"));
      const activeControllers = new Set([...active.values()].map((value) => value.controller));
      for (const candidate of controllers)
        if (!activeControllers.has(candidate)) candidate.abort(signal.reason);
      for (const value of active.values()) clearTimeout(value.startupTimer);
      options.signal?.removeEventListener("abort", aborted);
      for (const timer of timers.values()) clearTimeout(timer);
      for (const watcher of watches) {
        try {
          watcher.close();
        } catch (error) {
          watchFailures.push(error);
        }
      }
      const activeProcesses = new Set([...active.values()].map((value) => value.process));
      const activeDevs = new Set([...active.values()].map((value) => value.dev));
      const cleanup = await Promise.allSettled([
        ...[...owned.keys()].filter((child) => !activeProcesses.has(child)).map(stop),
        ...[...devs]
          .filter((dev) => !activeDevs.has(dev))
          .map((dev) => dev.close().finally(() => devs.delete(dev))),
      ]);
      await Promise.allSettled(jobs.values());
      // Dependents drain while their dependencies are still available.
      for (const service of [...services].reverse()) {
        const value = active.get(service.id);
        if (!value) continue;
        try {
          await stopActive(value);
        } catch (error) {
          watchFailures.push(error);
        } finally {
          active.delete(service.id);
        }
      }
      // Creation can settle during cancellation and add an owner after the first snapshot.
      const remaining = await Promise.allSettled([
        ...[...owned.keys()].map(stop),
        ...[...devs].map((dev) => dev.close().finally(() => devs.delete(dev))),
      ]);
      for (const service of services)
        emit({
          ...states.get(service.id),
          id: service.id,
          status: "stopped",
          version: versions.get(service.id) ?? 0,
          elapsedMs: states.get(service.id)?.elapsedMs ?? 0,
        });
      const failures = [
        ...watchFailures,
        ...[...cleanup, ...remaining]
          .filter((item) => item.status === "rejected")
          .map((item) => item.reason),
      ];
      const cleanupError = failures.length
        ? new AggregateError(failures, "Workspace cleanup failed")
        : undefined;
      finish(failure ?? cleanupError);
      if (cleanupError) throw cleanupError;
    }));
  }
  function aborted() {
    void close().catch(() => {});
  }
  options.signal?.addEventListener("abort", aborted, { once: true });
  try {
    if (options.signal?.aborted) controller.abort(options.signal.reason);
    if (options.workspace.prepare)
      await bounded(
        Promise.resolve().then(() => {
          signal.throwIfAborted();
          return options.workspace.prepare!({ signal });
        }),
        30000,
      );
    for (const service of services) {
      const startup = cycle(service, true);
      jobs.set(service.id, startup);
      try {
        await startup;
      } finally {
        jobs.delete(service.id);
      }
      signal.throwIfAborted();
    }
    for (const service of services) {
      if (!service.watch?.length) continue;
      const cwd = resolve(root, service.kind === "process" ? service.cwd : service.target.root);
      const watcher = new InputWatches(
        cwd,
        () => {
          clearTimeout(timers.get(service.id));
          timers.set(
            service.id,
            setTimeout(() => {
              if (!closed) void rebuild(service.id).catch((error) => fatal(service, error));
            }, 100),
          );
        },
        (error) => fatal(service, error),
      );
      watches.push(watcher);
      watcher.replace(service.watch.map((path) => resolve(cwd, path)));
    }
  } catch (error) {
    try {
      await close();
    } catch (cleanup) {
      throw new AggregateError([error, cleanup], "Workspace startup and cleanup failed", {
        cause: cleanup,
      });
    }
    throw error;
  }
  return Object.freeze({
    done,
    close,
    rebuild,
    get states() {
      return [...states.values()];
    },
  });
}
