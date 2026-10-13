import { createServer, connect } from "node:net";
import { resolve } from "node:path";
import { defineEnginePlugin } from "@lenso/engine/authoring";
import { RuntimeHost } from "@lenso/runtime-sdk";

export interface RuntimeExecutable {
  readonly binary: string;
  readonly args?: readonly string[];
  readonly ready: (host: RuntimeHost) => Promise<boolean>;
}

export interface RuntimeCompanionOptions {
  readonly kernelBinary: string;
  readonly services: readonly RuntimeExecutable[];
  readonly port?: number;
  readonly readinessTimeoutMs?: number;
  readonly shutdownTimeoutMs?: number;
  readonly signal?: AbortSignal;
}

type RegisterCleanup = (cleanup: () => Promise<void>) => unknown;

async function assertAvailable(port: number) {
  const server = createServer();
  await new Promise<void>((accept, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", () =>
      server.close((error) => (error ? reject(error) : accept())),
    );
  });
}

function listening(port: number): Promise<boolean> {
  return new Promise((accept) => {
    const socket = connect({ host: "127.0.0.1", port });
    const finish = (ready: boolean) => {
      socket.destroy();
      accept(ready);
    };
    socket.setTimeout(100, () => finish(false));
    socket.once("error", () => finish(false));
    socket.once("connect", () => finish(true));
  });
}

async function until(
  ready: () => Promise<boolean>,
  children: readonly ReturnType<typeof Bun.spawn>[],
  deadline: number,
  signal?: AbortSignal,
) {
  while (performance.now() < deadline) {
    signal?.throwIfAborted();
    for (const child of children) {
      if (child.exitCode !== null) throw new Error("Runtime companion exited before readiness.");
    }
    let timer: ReturnType<typeof setTimeout> | undefined;
    let onAbort: (() => void) | undefined;
    try {
      const result = await Promise.race([
        ready(),
        new Promise<never>((_, reject) => {
          timer = setTimeout(
            () => reject(new Error("Runtime companion readiness timed out.")),
            Math.max(1, deadline - performance.now()),
          );
        }),
        new Promise<never>((_, reject) => {
          onAbort = () => reject(signal?.reason);
          signal?.addEventListener("abort", onAbort, { once: true });
          if (signal?.aborted) onAbort();
        }),
      ]);
      signal?.throwIfAborted();
      if (result) return;
    } finally {
      clearTimeout(timer);
      if (onAbort) signal?.removeEventListener("abort", onAbort);
    }
    await Bun.sleep(25);
  }
  throw new Error("Runtime companion readiness timed out.");
}

async function stop(child: ReturnType<typeof Bun.spawn>, timeoutMs: number) {
  if (child.exitCode !== null) return;
  child.kill("SIGTERM");
  const timer = setTimeout(() => child.kill("SIGKILL"), timeoutMs);
  try {
    await child.exited;
  } finally {
    clearTimeout(timer);
  }
}

/** Callers register each acquired resource before the next fallible startup step. */
export async function startRuntimeCompanions(
  options: RuntimeCompanionOptions,
  onCleanup: RegisterCleanup,
) {
  const port = options.port ?? 7331;
  if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("Invalid runtime port.");
  const lifetime = new AbortController();
  const signal = options.signal
    ? AbortSignal.any([options.signal, lifetime.signal])
    : lifetime.signal;
  onCleanup(async () => {
    lifetime.abort(new Error("Runtime companions closed."));
  });
  const deadline = performance.now() + (options.readinessTimeoutMs ?? 10000);
  await assertAvailable(port);
  const spawn = (binary: string, args: readonly string[]) => {
    signal.throwIfAborted();
    const child = Bun.spawn([resolve(binary), ...args], {
      env: { ...process.env, LENSO_RUNTIME_PORT: String(port) },
      stdin: "ignore",
      stdout: "inherit",
      stderr: "inherit",
    });
    return child;
  };
  const kernel = spawn(options.kernelBinary, ["--listen", `127.0.0.1:${port}`]);
  onCleanup(async () => {
    lifetime.abort(new Error("Runtime companions closed."));
    await stop(kernel, options.shutdownTimeoutMs ?? 1000);
  });
  await until(() => listening(port), [kernel], deadline, signal);
  const probe = await RuntimeHost.connect({
    namespace: "lenso",
    service: "readiness",
    instance: crypto.randomUUID(),
    port,
    contract: { version: "1", methods: {} },
  });
  onCleanup(async () => {
    lifetime.abort(new Error("Runtime companions closed."));
    await probe.close();
  });
  const services: ReturnType<typeof Bun.spawn>[] = [];
  for (const service of options.services) {
    const index = services.length;
    services.push(spawn(service.binary, service.args ?? []));
    onCleanup(async () => {
      lifetime.abort(new Error("Runtime companions closed."));
      await stop(services[index], options.shutdownTimeoutMs ?? 1000);
    });
    await until(() => service.ready(probe), [kernel, ...services], deadline, signal);
  }
  let restart: Promise<void> = Promise.resolve();
  return {
    /** Rebuild externally first. Restarts one service, never the kernel or TypeScript graph. */
    restartService(index: number): Promise<void> {
      const service = options.services[index];
      if (!service) return Promise.reject(new Error("Unknown runtime companion service."));
      const next = restart.then(async () => {
        signal.throwIfAborted();
        await stop(services[index], options.shutdownTimeoutMs ?? 1000);
        services[index] = spawn(service.binary, service.args ?? []);
        await until(
          () => service.ready(probe),
          [kernel, ...services],
          performance.now() + (options.readinessTimeoutMs ?? 10000),
          signal,
        );
      });
      restart = next.catch(() => {});
      return next;
    },
  };
}

export function runtimeEnginePlugin(
  options: RuntimeCompanionOptions & {
    readonly name?: string;
    /** false borrows companions owned by an outer dev launcher. */
    readonly startCompanions?: boolean;
  },
) {
  return defineEnginePlugin({
    name: options.name ?? "lenso/runtime",
    setup(context) {
      context.dev("runtime-companions", async (event) => {
        if (event === "beforeStart" && options.startCompanions !== false) {
          await startRuntimeCompanions(options, (cleanup) => context.onCleanup(cleanup));
        }
      });
    },
  });
}
