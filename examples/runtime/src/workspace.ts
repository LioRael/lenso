import {
  defineWorkspace,
  generate,
  EngineError,
  type WorkspaceProcessContext,
} from "@lenso/engine";
import { RuntimeError, RuntimeHost } from "@lenso/runtime-sdk";
import { createServer } from "node:net";
import { resolve } from "node:path";

export interface RuntimeWorkspaceOptions {
  runtimeRoot: string;
  rustRoot: string;
  appRoot?: string;
  port?: number;
  implementation?: "rust" | "ts";
}

export function runtimeWorkspace(options: RuntimeWorkspaceOptions) {
  const appRoot = resolve(options.appRoot ?? resolve(import.meta.dir, ".."));
  const runtimeRoot = resolve(options.runtimeRoot);
  const rustRoot = resolve(options.rustRoot);
  const port = options.port ?? 7331;
  const implementation = options.implementation ?? "rust";
  if (!Number.isInteger(port) || port < 1 || port > 65535)
    throw new Error("LENSO_RUNTIME_PORT must be a TCP port in 1..65535.");
  const environment = {
    LENSO_RUNTIME_ROOT: runtimeRoot,
    LENSO_RUNTIME_PORT: String(port),
    LENSO_RUNTIME_EXTERNAL: "1",
  };
  const probe = async () =>
    RuntimeHost.connect({
      namespace: "lenso",
      service: "workspace-observer",
      instance: crypto.randomUUID(),
      port,
      contract: { version: "1", methods: {} },
    });
  async function ready(context: WorkspaceProcessContext) {
    let host: RuntimeHost | undefined;
    try {
      while (!host) {
        context.signal.throwIfAborted();
        if (context.process.exitCode !== null)
          throw new Error("Runtime process exited before ready.");
        try {
          host = await probe();
        } catch {
          context.signal.throwIfAborted();
          await Bun.sleep(25);
        }
      }
      while (true) {
        context.signal.throwIfAborted();
        if (context.process.exitCode !== null)
          throw new Error("Batch process exited before ready.");
        const status = await host.status();
        if (
          status.services.some(
            (service) =>
              service.namespace === "demo" &&
              service.service === "batch" &&
              service.instance === context.instance &&
              service.state === "ready",
          )
        )
          return { urls: [`tcp://127.0.0.1:${port}/demo/batch`] };
        await Bun.sleep(25);
      }
    } finally {
      await host?.close();
    }
  }
  async function activate(context: WorkspaceProcessContext) {
    const host = await probe();
    try {
      context.signal.throwIfAborted();
      await host.promote({
        namespace: "demo",
        service: "batch",
        instance: context.instance,
        timeoutMs: 5000,
      });
    } catch (error) {
      if (error instanceof RuntimeError) {
        const confirmedRejection = [
          "contract_mismatch",
          "not_ready",
          "service_not_found",
          "invalid_input",
          "overloaded",
        ].includes(error.code);
        if (!confirmedRejection) {
          let observer: RuntimeHost | undefined;
          try {
            observer = await probe();
            const snapshot = await observer.status({ timeoutMs: 2000 });
            const selected = snapshot.services.find(
              (service) =>
                service.namespace === "demo" && service.service === "batch" && service.selected,
            );
            if (selected?.instance === context.instance && selected.state === "ready") return;
          } catch {
          } finally {
            await observer?.close();
          }
          throw new EngineError(
            {
              code: "workspace-cutover-unknown",
              phase: "workspace-dev",
              message:
                "Runtime activation outcome is unknown; stop the workspace rather than discard a possibly selected candidate.",
            },
            { cause: error },
          );
        }
        throw new EngineError(
          {
            code: error.code,
            phase: "workspace-dev",
            message:
              "Runtime rejected candidate activation; the previous instance remains selected.",
          },
          { cause: error },
        );
      }
      throw error;
    } finally {
      await host.close();
    }
  }
  const builder = resolve(rustRoot, "scripts/build-runtime.sh");
  return defineWorkspace({
    async prepare({ signal }) {
      const reservation = createServer();
      await new Promise<void>((accept, reject) => {
        reservation.once("error", (error) =>
          reject(
            new EngineError(
              {
                code:
                  (error as NodeJS.ErrnoException).code === "EADDRINUSE"
                    ? "workspace-port-in-use"
                    : "workspace-port-unavailable",
                phase: "workspace-dev",
                message: `Cannot reserve local Runtime port ${port}; no existing process was adopted or stopped.`,
              },
              { cause: error },
            ),
          ),
        );
        reservation.listen(port, "127.0.0.1", () =>
          reservation.close((error) => (error ? reject(error) : accept())),
        );
      });
      signal.throwIfAborted();
      await generate(appRoot);
      signal.throwIfAborted();
    },
    services: [
      {
        id: "runtime",
        kind: "process",
        cwd: runtimeRoot,
        build: ["cargo", "build", "--locked", "-p", "kernel", "--bin", "lenso-runtime"],
        command: [
          resolve(runtimeRoot, "target/debug/lenso-runtime"),
          "--listen",
          `127.0.0.1:${port}`,
        ],
        startupTimeoutMs: 120000,
        stopTimeoutMs: 8000,
        async ready(context) {
          while (true) {
            context.signal.throwIfAborted();
            if (context.process.exitCode !== null) throw new Error("Kernel startup failed.");
            try {
              const host = await probe();
              await host.close();
              return { urls: [`tcp://127.0.0.1:${port}`] };
            } catch {
              context.signal.throwIfAborted();
              await Bun.sleep(25);
            }
          }
        },
      },
      implementation === "rust"
        ? {
            id: "batch",
            kind: "process",
            dependsOn: ["runtime"],
            cwd: rustRoot,
            env: environment,
            build: [
              "sh",
              builder,
              "--runtime-root",
              runtimeRoot,
              "--out",
              "target/debug/logs-batch",
            ],
            command: [resolve(rustRoot, "target/debug/logs-batch")],
            watch: [
              resolve(rustRoot, "Cargo.toml"),
              resolve(rustRoot, "Cargo.lock"),
              resolve(rustRoot, "crates"),
              resolve(rustRoot, "examples/batch"),
              builder,
              resolve(rustRoot, "scripts/stage-native.ts"),
              resolve(runtimeRoot, "examples/contracts"),
              resolve(runtimeRoot, "sdk/typescript/generate.ts"),
              resolve(runtimeRoot, "crates/sdk"),
              resolve(runtimeRoot, "crates/protocol"),
            ],
            ready,
            activate,
            startupTimeoutMs: 120000,
            stopTimeoutMs: 8000,
          }
        : {
            id: "batch",
            kind: "process",
            dependsOn: ["runtime"],
            cwd: appRoot,
            env: environment,
            build: [process.execPath, resolve(appRoot, "src/build-ts-batch.ts")],
            command: [process.execPath, resolve(appRoot, ".lenso/ts-batch/server.js")],
            watch: [
              resolve(appRoot, "src/aggregate.ts"),
              resolve(appRoot, "src/batch-app.ts"),
              resolve(appRoot, "src/ts-batch.ts"),
              resolve(appRoot, "src/build-ts-batch.ts"),
              resolve(appRoot, "src/runtime-contract.ts"),
              resolve(appRoot, "src/logs.contract.json"),
              resolve(appRoot, "lenso.engine.ts"),
              resolve(runtimeRoot, "sdk/typescript/generate.ts"),
            ],
            ready,
            activate,
            stopTimeoutMs: 8000,
          },
      {
        id: "frontend",
        kind: "application",
        dependsOn: ["batch"],
        target: { root: appRoot },
        env: environment,
      },
    ],
  });
}
