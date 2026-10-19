import { installedOperationVersions } from "./provenance";
import { lstat, open, rm } from "node:fs/promises";
import { constants } from "node:fs";
import { dirname, resolve } from "node:path";
import type { RunningApp, Plugin } from "@lenso/core";
import {
  describeOperation,
  operationError,
  invokeValidatedOperation,
  resolveOperation,
  validateOperationInput,
  validateOperations,
  type Operation,
  type OperationBoundOptions,
  type OperationInvocationOptions,
  redactOperationDescription,
} from "@lenso/engine/operations";
import { EngineError, diagnostic, redact, environmentSecrets } from "@lenso/engine/diagnostics";

function refuse(code: string, message: string): never {
  throw new EngineError({ code, phase: "invoke", message, effectState: "not-started" });
}

export type DevOperationBinding<O extends Operation> = (
  operation: O,
  input: unknown,
  running: RunningApp,
  request: { readonly signal: AbortSignal; readonly generation: string },
) => OperationBoundOptions<O> | Promise<OperationBoundOptions<O>>;

export interface DevCallRecord {
  readonly callRef: string;
  readonly id: string;
  readonly operation: { readonly pluginId: string; readonly method: string };
  readonly instanceId: string;
  readonly generation: string;
  readonly codeRevision: unknown;
  readonly pluginId: string;
  readonly method: string;
  readonly startedAt: number;
  readonly durationMs: number;
  readonly code: string;
  readonly phase: string;
  readonly effectState: string;
  readonly outcome: "success" | "failure";
}

/** Explicit local developer entry. Borrows exactly the app also supplied to Web/MCP.
 * No listener is created outside a supervisor that opted into --connect. */
export async function serveDevOperations<O extends Operation>(options: {
  running: RunningApp;
  plugins: readonly Plugin<unknown>[];
  operations: readonly O[];
  binding?: DevOperationBinding<O>;
}) {
  const generation = process.env.LENSO_DEV_GENERATION;
  if (!generation || !process.env.LENSO_DEV_DESCRIPTOR || !process.env.LENSO_DEV_SOCKET)
    return undefined;
  const operations = validateOperations(options.plugins, options.operations).map((operation) =>
    Object.freeze({
      ...operation,
      source: operation.source ?? { file: process.env.LENSO_DEV_CONFIG ?? "lenso.config.ts" },
    }),
  );
  for (const plugin of options.plugins) options.running.get(plugin);
  const socket = process.env.LENSO_DEV_SOCKET;
  const directory = dirname(socket);
  let ready = false;
  let closed = false;
  let closing: Promise<void> | undefined;
  let pending: Promise<Response> | undefined;
  const controllers = new Set<AbortController>();
  const recent: { -readonly [K in keyof DevCallRecord]: DevCallRecord[K] }[] = [];
  const codeRevision = process.env.LENSO_DEV_PROVENANCE
    ? JSON.parse(process.env.LENSO_DEV_PROVENANCE).codeRevision
    : null;
  const onMessage = (value: unknown) => {
    if (value && typeof value === "object" && Reflect.get(value, "generation") === generation) {
      if (Reflect.get(value, "type") === "lenso:dev-admit" && !closed) ready = true;
      if (Reflect.get(value, "type") === "lenso:dev-revoke") {
        ready = false;
        for (const controller of controllers) controller.abort();
      }
    }
  };
  process.on("message", onMessage);
  const server = Bun.serve({
    unix: socket,
    maxRequestBodySize: 256 * 1024,
    async fetch(request) {
      if (
        request.method !== "POST" ||
        new URL(request.url).pathname !== "/operations" ||
        request.headers.has("origin")
      )
        return new Response(null, { status: 403 });
      if (request.headers.get("x-lenso-generation") !== generation)
        return Response.json({
          ok: false,
          error: {
            code: "stale-generation",
            phase: "invoke",
            message: "Development generation changed; inspect the selected instance again.",
            effectState: "not-started",
          },
        });
      if (!ready || closed)
        return Response.json({
          ok: false,
          error: {
            code: "dev-not-ready",
            phase: "invoke",
            message: "Selected development instance is not ready.",
            effectState: "not-started",
          },
        });
      if (pending)
        return Response.json({
          ok: false,
          error: {
            code: "adapter-busy",
            phase: "invoke",
            message: "Development instance is busy; no request was queued.",
            effectState: "not-started",
          },
        });
      const execute = async () => {
        const controller = new AbortController();
        controllers.add(controller);
        const signal = AbortSignal.any([
          controller.signal,
          request.signal,
          AbortSignal.timeout(30_000),
        ]);
        let dispatched = false;
        let row: (typeof recent)[number] | undefined;
        try {
          const body = await request.json();
          if (body.action === "inspect")
            return Response.json({
              ok: true,
              data: {
                inspection: "running",
                installedVersions: installedOperationVersions(),
                scope: {
                  checked: ["declared-operations", "ready-generation"],
                  notChecked: ["provider-health", "business-authorization"],
                },
                instanceId: options.running.instanceId,
                generation,
                operations: operations
                  .filter(
                    (operation) =>
                      (!body.pluginId || operation.plugin.id === body.pluginId) &&
                      (!body.method || operation.method === body.method),
                  )
                  .map((operation) =>
                    redactOperationDescription(
                      describeOperation(
                        operation,
                        process.env.LENSO_DEV_CONFIG ?? "lenso.config.ts",
                      ),
                    ),
                  ),
                capabilities: {
                  connectedCall: true,
                  recentCalls: true,
                  automaticRetry: false,
                  durableHistory: false,
                },
              },
            });
          if (body.action === "recent") {
            const cutoff = Date.now() - 300_000;
            const limit = body.limit ?? 16;
            if (!Number.isSafeInteger(limit) || limit < 1 || limit > 64)
              refuse("invalid-arguments", "Recent-call limit must be between 1 and 64.");
            if (body.method && !body.pluginId)
              refuse("invalid-arguments", "Method selection requires a plugin ID.");
            if (body.pluginId && body.method)
              resolveOperation(options.plugins, operations, body.pluginId, body.method);
            while (recent[0] && recent[0].startedAt < cutoff) recent.shift();
            return Response.json({
              ok: true,
              data: redact(
                {
                  generation,
                  retention: { maxEntries: 64, maxAgeMs: 300_000, durable: false, payloads: false },
                  calls: recent
                    .filter(
                      (record) =>
                        record.startedAt >= cutoff &&
                        (!body.pluginId || record.pluginId === body.pluginId) &&
                        (!body.method || record.method === body.method),
                    )
                    .slice(-limit),
                },
                environmentSecrets(),
              ),
            });
          }
          if (
            body.action !== "call" ||
            typeof body.pluginId !== "string" ||
            typeof body.method !== "string"
          )
            refuse(
              "unsupported",
              "Only inspect, call and recent are supported by this local entry.",
            );
          const operation = resolveOperation(
            options.plugins,
            operations,
            body.pluginId,
            body.method,
          ) as O;
          row = {
            id: crypto.randomUUID(),
            callRef: "",
            operation: { pluginId: operation.plugin.id, method: operation.method },
            instanceId: options.running.instanceId,
            generation,
            codeRevision,
            outcome: "success",
            pluginId: operation.plugin.id,
            method: operation.method,
            startedAt: Date.now(),
            durationMs: 0,
            code: "success",
            phase: "invoke",
            effectState: "not-started",
          };
          row.callRef = row.id;
          const input = await validateOperationInput(operation, body.input);
          let binding: OperationInvocationOptions;
          try {
            binding = options.binding
              ? await options.binding(operation, input, options.running, { signal, generation })
              : {};
          } catch (error) {
            throw operationError(operation, error);
          }
          const data = await invokeValidatedOperation<Operation>(
            options.running,
            operation,
            input,
            {
              ...binding,
              signal: binding.signal ? AbortSignal.any([signal, binding.signal]) : signal,
              beforeExecute: async () => {
                await binding.beforeExecute?.();
                if (!ready || closed) refuse("dev-not-ready", "Development instance was revoked.");
                dispatched = true;
              },
            },
          );
          row.effectState = "completed";
          return Response.json({ ok: true, data });
        } catch (error) {
          const detail = {
            ...diagnostic(
              error,
              row
                ? {
                    pluginId: row.pluginId,
                    operation: `${row.pluginId}.${row.method}`,
                    source: { file: process.env.LENSO_DEV_CONFIG ?? "lenso.config.ts" },
                  }
                : {},
            ),
            effectState: dispatched ? "unknown" : "not-started",
          };
          if (row) {
            row.outcome = "failure";
            row.code = detail.code;
            row.phase = detail.phase;
            row.effectState = detail.effectState;
          }
          return Response.json({ ok: false, error: redact(detail, environmentSecrets()) });
        } finally {
          controllers.delete(controller);
          if (row) {
            row.durationMs = Date.now() - row.startedAt;
            recent.push(row);
            if (recent.length > 64) recent.shift();
          }
        }
      };
      pending = execute();
      try {
        return await pending;
      } finally {
        pending = undefined;
      }
    },
  });
  return {
    connection: { socket, generation, instanceId: options.running.instanceId },
    close() {
      return (closing ??= (async () => {
        closed = true;
        ready = false;
        process.off("message", onMessage);
        for (const controller of controllers) controller.abort();
        await pending;
        await server.stop(true);
        recent.length = 0;
        await rm(directory, { recursive: true, force: true });
      })());
    },
  };
}

/** Connect only to an explicitly selected same-user private local descriptor. Never falls back to a fresh app. */
export async function connectDev(
  file: string,
  action: "inspect" | "call" | "recent",
  options: {
    generation?: string;
    pluginId?: string;
    method?: string;
    input?: unknown;
    limit?: number;
    signal?: AbortSignal;
  } = {},
) {
  const path = resolve(file);
  let descriptor;
  let handle;
  try {
    handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    const stat = await handle.stat();
    if (
      !stat.isFile() ||
      (stat.mode & 0o077) !== 0 ||
      stat.uid !== process.getuid?.() ||
      stat.size > 16384
    )
      refuse(
        "unsafe-dev-connection",
        "Development descriptor must be a private same-user regular file.",
      );
    descriptor = JSON.parse(await handle.readFile("utf8"));
  } catch (error) {
    if (error instanceof EngineError) throw error;
    refuse(
      "dev-connection-unavailable",
      "Cannot read the explicitly selected private development descriptor.",
    );
  } finally {
    await handle?.close();
  }
  if (
    descriptor.schemaVersion !== 1 ||
    typeof descriptor.socket !== "string" ||
    typeof descriptor.generation !== "string"
  )
    refuse("unsupported-dev-connection", "Unsupported development connection descriptor.");
  if ((action === "call" || action === "recent") && !options.generation)
    refuse(
      "generation-required",
      "Connected calls and recent queries require the generation returned by inspect.",
    );
  if (options.generation && options.generation !== descriptor.generation)
    refuse(
      "stale-generation",
      "Development generation changed; inspect the selected instance again.",
    );
  const socket = await lstat(descriptor.socket);
  const directory = await lstat(dirname(descriptor.socket));
  if (
    !socket.isSocket() ||
    socket.uid !== process.getuid?.() ||
    !directory.isDirectory() ||
    directory.isSymbolicLink() ||
    directory.uid !== process.getuid?.() ||
    (directory.mode & 0o077) !== 0
  )
    refuse(
      "unsafe-dev-connection",
      "Development socket must belong to a private same-user directory.",
    );
  try {
    const response = await fetch("http://localhost/operations", {
      unix: descriptor.socket,
      method: "POST",
      headers: {
        "x-lenso-generation": options.generation ?? descriptor.generation,
        "content-type": "application/json",
      },
      body: JSON.stringify({
        action,
        pluginId: options.pluginId,
        method: options.method,
        input: options.input,
        limit: options.limit,
      }),
      signal: options.signal ?? AbortSignal.timeout(35_000),
    });
    const reader = response.body?.getReader();
    const chunks: Uint8Array[] = [];
    let size = 0;
    try {
      if (reader)
        while (true) {
          const item = await reader.read();
          if (item.done) break;
          size += item.value.length;
          if (size > 2 * 1024 * 1024) {
            await reader.cancel();
            throw new EngineError({
              code: "output-too-large",
              phase: "output",
              message: "Development response exceeds the entry limit.",
              effectState: action === "call" ? "unknown" : "not-started",
            });
          }
          chunks.push(item.value);
        }
    } finally {
      reader?.releaseLock();
    }
    const body = Buffer.concat(chunks).toString("utf8");
    const result = JSON.parse(body);
    if (!result.ok) throw new EngineError(result.error);
    return action === "inspect"
      ? {
          ...result.data,
          provenance: {
            ...descriptor.provenance,
            installedVersions: result.data.installedVersions,
          },
        }
      : result.data;
  } catch (error) {
    if (error instanceof EngineError) throw error;
    throw new EngineError({
      code: "dev-connection-lost",
      phase: "invoke",
      message:
        "Development connection lost; a submitted call may have run. Inspect current state before retrying.",
      effectState: action === "call" ? "unknown" : "not-started",
    });
  }
}
