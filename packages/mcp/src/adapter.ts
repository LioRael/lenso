import { createHash } from "node:crypto";
import type { Plugin } from "@lenso/core";
import { createAgentTools, createManageSelection, type ManageSelection } from "@lenso/manage";
import {
  boundedJson,
  type Operation,
  type OperationBoundOptions,
  type OperationRuntime,
  type OperationExecutionEvidence,
} from "@lenso/engine/operations";
import { diagnostic, environmentSecrets, redact, EngineError } from "@lenso/engine/diagnostics";
import { diagnosticResult, executionResult } from "./result";
import {
  ErrorCode,
  McpError,
  ToolSchema,
  type CallToolResult,
  type Tool,
} from "@modelcontextprotocol/sdk/types.js";

export interface McpRequestContext<I> {
  readonly identity: I;
  readonly requestId: string;
  readonly signal: AbortSignal;
}

export interface McpAdapterOptions<I, O extends Operation = Operation> {
  readonly running: OperationRuntime;
  readonly plugins: readonly Plugin<unknown>[];
  readonly operations: readonly O[];
  readonly canList: (operation: O, request: McpRequestContext<I>) => boolean | Promise<boolean>;
  readonly authorize: (operation: O, request: McpRequestContext<I>) => boolean | Promise<boolean>;
  readonly binding: (
    operation: O,
    input: unknown,
    request: McpRequestContext<I>,
  ) => OperationBoundOptions<NoInfer<O>> | Promise<OperationBoundOptions<NoInfer<O>>>;
  readonly maxInputBytes?: number;
  readonly maxOutputBytes?: number;
  readonly maxCatalogBytes?: number;
  readonly maxConcurrentCalls?: number;
  readonly requestTimeoutMs?: number;
  /** Visible tools per page (default 100). Cursors are single-use, expire after five minutes,
   * and are capped at 1024 outstanding per adapter. Restart listing after a visibility change. */
  readonly listPageSize?: number;
  /** Stable trusted identity key; otherwise cursors require the same identity reference. */
  readonly identityKey?: (identity: I) => string;
}

/** Hash the exact tuple, not redacted metadata or a lossy punctuation-normalized label. */
export function operationToolName(pluginId: string, method: string): string {
  return `operation_${createHash("sha256")
    .update(JSON.stringify([pluginId, method]))
    .digest("base64url")}`;
}

const messages: Record<string, string> = {
  "invalid-input": "Input does not satisfy the operation schema.",
  "input-too-large": "Operation input exceeds the host limit.",
  "output-too-large": "Operation output exceeds the host limit.",
  "serialization-failed": "Operation did not return finite JSON.",
  "confirmation-required": "Required confirmation is unavailable.",
  "approval-required": "Required approval is unavailable.",
  "missing-context-binding": "Required operation context is unavailable.",
  "forbidden-operation": "Operation is not available to this caller.",
  "unavailable-operation": "Operation is unavailable.",
  "invocation-failed": "Operation invocation failed.",
  "request-cancelled": "Request cancelled; external effects may have occurred.",
  "request-timeout": "Request timed out; work may continue and external effects may have occurred.",
  "adapter-busy": "Adapter is busy; no request was queued.",
  "adapter-closed": "Adapter is closed.",
  UNAUTHORIZED: "Authentication required.",
  REAUTHENTICATION_REQUIRED: "Reauthentication required.",
  FORBIDDEN: "Access denied.",
  SERVICE_UNAVAILABLE: "Service unavailable.",
  "resource-not-found": "Resource not found.",
  "not-found": "Resource not found.",
  forbidden: "Access denied.",
  "permission-denied": "Access denied.",
  "authorization-denied": "Access denied.",
  conflict: "Operation conflicts with current state.",
  "deduplication-conflict": "Operation conflicts with current state.",
  "invalid-key": "Invalid operation input.",
  "invalid-task": "Invalid operation input.",
  "invalid-options": "Invalid operation input.",
  "too-large": "Operation exceeds the service limit.",
  "provider-unavailable": "Service unavailable.",
  closed: "Service unavailable.",
  provider: "Provider operation failed.",
  unsupported: "Operation is unsupported.",
  aborted: "Request cancelled; external effects may have occurred.",
};

function positive(value: number | undefined, fallback: number, min = 1): number {
  const actual = value ?? fallback;
  if (!Number.isSafeInteger(actual) || actual < min) throw new TypeError("Invalid MCP limit.");
  return actual;
}

function failure(code: string, error?: unknown, structured = true): CallToolResult {
  const safe = Object.hasOwn(messages, code) ? code : "invocation-failed";
  const phase =
    safe === "invalid-input" || safe === "input-too-large"
      ? "input"
      : safe === "output-too-large" || safe === "serialization-failed"
        ? "output"
        : "invoke";
  return diagnosticResult(
    JSON.stringify(
      error instanceof EngineError
        ? redact(diagnostic(error), environmentSecrets())
        : { code: safe, phase, message: messages[safe], effectState: "unknown" },
    ),
    structured,
  );
}

/** Borrows only runtime capabilities, never app start/stop ownership. */
export async function createMcpAdapter<I, O extends Operation = Operation>(
  options: McpAdapterOptions<I, O>,
) {
  const { running, plugins, operations, ...policies } = options;
  const selection = createManageSelection({ running, plugins, operations });
  try {
    return await adapterForSelection(selection, policies);
  } catch (error) {
    selection.close();
    throw error;
  }
}

async function adapterForSelection<I, O extends Operation>(
  selection: ManageSelection<O>,
  options: Omit<McpAdapterOptions<I, O>, "running" | "plugins" | "operations">,
) {
  const maxInput = positive(options.maxInputBytes, 256 * 1024);
  const maxOutput = positive(options.maxOutputBytes, 1024 * 1024, 256);
  const maxCatalog = positive(options.maxCatalogBytes, 256 * 1024);
  const maxConcurrent = positive(options.maxConcurrentCalls, 4);
  const timeout = positive(options.requestTimeoutMs, 30_000);
  const pageSize = positive(options.listPageSize, 100);
  if ([options.binding, options.canList, options.authorize].some((fn) => typeof fn !== "function"))
    throw new TypeError("MCP requires trusted binding, catalog and invocation policies.");
  const names = new Map<O, string>();
  const canonical = selection.createAdapter({
    canList: (operation) => {
      names.set(operation, operationToolName(operation.plugin.id, operation.method));
      return true;
    },
    binding: () => {
      throw new Error("Metadata adapter cannot invoke.");
    },
    maxOutputBytes: maxCatalog,
  });
  const [catalog, agentTools] = await Promise.all([
    canonical.catalog(),
    createAgentTools(canonical),
  ]);
  const exactNames = [...names.values()];
  const tools = agentTools.map((item, index): Tool => {
    const operation = catalog[index]!;
    const tool: Tool = {
      name: exactNames[index]!,
      title: `${operation.pluginId}.${operation.method}`,
      description: [
        item.description,
        ...(operation.outputDescription ? [`Output: ${operation.outputDescription}`] : []),
        "Cancellation signals reach host bindings. Only cooperative services interrupt; cancellation does not roll back effects.",
      ].join("\n"),
      inputSchema: item.inputSchema as Tool["inputSchema"],
      ...(operation.outputSchema
        ? {
            outputSchema: (operation.outputSchema.type === "object"
              ? operation.outputSchema
              : {
                  type: "object",
                  properties: { result: operation.outputSchema },
                  required: ["result"],
                  additionalProperties: false,
                }) as Tool["outputSchema"],
          }
        : {}),
      _meta: { "lenso/operation": operation },
      execution: { taskSupport: "forbidden" },
      annotations: {
        readOnlyHint: operation.effect === "read",
        destructiveHint: operation.destructive ?? true,
        idempotentHint: operation.retry === "safe",
        openWorldHint: true,
      },
    };
    if (!ToolSchema.safeParse(tool).success)
      throw new TypeError("Operation cannot be represented as an MCP tool.");
    return tool;
  });
  boundedJson({ tools }, maxCatalog);
  const toolByKey = new Map(catalog.map((operation, index) => [operation.key, tools[index]!]));
  const entryByName = new Map(tools.map((tool, index) => [tool.name, catalog[index]!]));
  if (entryByName.size !== tools.length) throw new TypeError("MCP tool name collision.");
  const cursors = new Map<
    string,
    {
      identity: I | string;
      visible: string;
      offset: number;
      expires: number;
    }
  >();
  let closed = false;
  let closePromise: Promise<void> | undefined;
  const pending = new Set<Promise<unknown>>();
  const controllers = new Set<AbortController>();

  function tracked<T>(
    request: McpRequestContext<I>,
    action: (context: McpRequestContext<I>) => Promise<T>,
  ): Promise<T> {
    const controller = new AbortController();
    const signal = AbortSignal.any([request.signal, controller.signal]);
    controllers.add(controller);
    let timer: ReturnType<typeof setTimeout>;
    let abort: () => void;
    let timedOut = false;
    const interrupted = new Promise<never>((_, reject) => {
      abort = () =>
        reject(
          new EngineError({
            code: timedOut ? "request-timeout" : "request-cancelled",
            phase: "invoke",
            message: "MCP request interrupted.",
          }),
        );
      signal.addEventListener("abort", abort, { once: true });
      if (signal.aborted) abort();
      timer = setTimeout(() => {
        timedOut = true;
        controller.abort();
      }, timeout);
    });
    const work = Promise.resolve().then(() => {
      signal.throwIfAborted();
      return action({ ...request, signal });
    });
    pending.add(work);
    const cleanup = () => {
      clearTimeout(timer);
      signal.removeEventListener("abort", abort);
      controllers.delete(controller);
      pending.delete(work);
    };
    // A timeout releases the response, not the execution slot or lifecycle drain.
    void work.then(cleanup, cleanup);
    return Promise.race([work, interrupted]);
  }

  return Object.freeze({
    async listTools(
      request: McpRequestContext<I>,
      cursor?: string,
    ): Promise<{ tools: Tool[]; nextCursor?: string }> {
      if (closed) throw new McpError(ErrorCode.InvalidRequest, "Adapter is closed.");
      if (pending.size >= maxConcurrent)
        throw new McpError(ErrorCode.InternalError, "Adapter is busy.");
      try {
        return await tracked(request, async (context) => {
          const scoped = selection.createAdapter({
            maxOutputBytes: maxCatalog,
            binding: () => {
              throw new Error("Metadata adapter cannot invoke.");
            },
            canList: async (operation) => {
              const visible = await options.canList(operation, context);
              context.signal.throwIfAborted();
              return visible;
            },
          });
          const visible = (await scoped.catalog()).map((operation) =>
            toolByKey.get(operation.key)!,
          );
          context.signal.throwIfAborted();
          const now = Date.now();
          for (const [key, state] of cursors) if (state.expires <= now) cursors.delete(key);
          const identity = options.identityKey
            ? options.identityKey(context.identity)
            : context.identity;
          const fingerprint = createHash("sha256")
            .update(JSON.stringify(visible.map((tool) => tool.name)))
            .digest("hex");
          const state = cursor === undefined ? undefined : cursors.get(cursor);
          if (
            cursor !== undefined &&
            (!state || state.identity !== identity || state.visible !== fingerprint)
          )
            throw new McpError(ErrorCode.InvalidParams, "Invalid or stale tool cursor.");
          const offset = state?.offset ?? 0;
          const page: { tools: Tool[]; nextCursor?: string } = {
            tools: visible.slice(offset, offset + pageSize),
          };
          if (offset + pageSize < visible.length) {
            if (cursors.size - (state ? 1 : 0) >= 1024)
              throw new McpError(ErrorCode.InternalError, "Tool cursor capacity reached.");
            page.nextCursor = crypto.randomUUID();
          }
          // Return a fresh finite copy, preventing callers mutating subsequent discovery.
          const result = JSON.parse(boundedJson(page, maxCatalog)) as typeof page;
          if (cursor !== undefined) cursors.delete(cursor);
          if (page.nextCursor)
            cursors.set(page.nextCursor, {
              identity,
              visible: fingerprint,
              offset: offset + pageSize,
              expires: now + 5 * 60_000,
            });
          return result;
        });
      } catch (error) {
        if (error instanceof McpError) throw error;
        throw new McpError(ErrorCode.InternalError, "Tool discovery failed or was interrupted.");
      }
    },
    async callTool(
      name: string,
      input: unknown,
      request: McpRequestContext<I>,
    ): Promise<CallToolResult> {
      const entry = entryByName.get(name);
      if (!entry) throw new McpError(ErrorCode.InvalidParams, "Unknown tool.");
      const structured = !toolByKey.get(entry.key)!.outputSchema;
      const fail = (code: string, error?: unknown) => failure(code, error, structured);
      if (closed) return fail("adapter-closed");
      if (pending.size >= maxConcurrent) return fail("adapter-busy");
      if (request.signal.aborted) return fail("request-cancelled");
      try {
        boundedJson(input, maxInput);
      } catch (error) {
        return fail(
          error instanceof EngineError && error.diagnostic.code === "output-too-large"
            ? "input-too-large"
            : "invalid-input",
        );
      }
      let evidence: OperationExecutionEvidence | undefined;
      const finish = (result: CallToolResult) => executionResult(result, evidence, maxOutput);
      try {
        const value = await tracked(request, async (context) => {
          const scoped = selection.createAdapter({
            maxOutputBytes: maxOutput,
            canList: async (operation) => {
              const visible = await options.canList(operation, context);
              const allowed =
                visible === true && (await options.authorize(operation, context)) === true;
              context.signal.throwIfAborted();
              return allowed;
            },
            binding: async (operation, validated) => {
              const binding = await options.binding(operation, validated, context);
              context.signal.throwIfAborted();
              return {
                ...binding,
                onExecutionEvidence: (actual) => {
                  evidence = actual;
                  return binding.onExecutionEvidence?.(actual);
                },
                signal: binding.signal
                  ? AbortSignal.any([context.signal, binding.signal])
                  : context.signal,
              };
            },
          });
          const result = await scoped.invokeEntry(entry.key, input);
          context.signal.throwIfAborted();
          return result;
        });
        const text = boundedJson(value, maxOutput);
        const finite: unknown = JSON.parse(text);
        const structuredContent =
          finite !== null && typeof finite === "object" && !Array.isArray(finite)
            ? (finite as Record<string, unknown>)
            : { result: finite };
        boundedJson(structuredContent, maxOutput);
        return finish({ content: [{ type: "text", text }], structuredContent });
      } catch (error) {
        const failed = fail(
          error instanceof EngineError ? error.diagnostic.code : "invocation-failed",
          error,
        );
        const text = failed.content[0];
        if (text?.type === "text" && Buffer.byteLength(text.text) > maxOutput) {
          const detail = diagnostic(error);
          const compact = JSON.stringify({
            code: detail.code,
            phase: detail.phase,
            message: "Diagnostic truncated to the host output limit.",
            effectState: detail.effectState ?? "unknown",
            truncated: true,
          });
          return finish(
            Buffer.byteLength(compact) <= maxOutput
              ? diagnosticResult(compact, structured)
              : fail("output-too-large"),
          );
        }
        return finish(failed);
      }
    },
    close(): Promise<void> {
      if (closePromise) return closePromise;
      closed = true;
      cursors.clear();
      for (const controller of controllers) controller.abort();
      closePromise = Promise.allSettled(pending).then(() => {
        selection.close();
      });
      return closePromise;
    },
  });
}
