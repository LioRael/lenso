import type { Plugin } from "@lenso/core";
import { createAgentTools, createManageSelection, type ManageSelection } from "@lenso/manage";
import {
  boundedJson,
  type Operation,
  type OperationBoundOptions,
  type OperationRuntime,
} from "@lenso/engine/operations";
import { EngineError } from "@lenso/engine/diagnostics";
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

function failure(code: string): CallToolResult {
  const safe = Object.hasOwn(messages, code) ? code : "invocation-failed";
  const phase =
    safe === "invalid-input" || safe === "input-too-large"
      ? "input"
      : safe === "output-too-large" || safe === "serialization-failed"
        ? "output"
        : "invoke";
  return {
    isError: true,
    content: [
      { type: "text", text: JSON.stringify({ code: safe, phase, message: messages[safe] }) },
    ],
  };
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
  if ([options.binding, options.canList, options.authorize].some((fn) => typeof fn !== "function"))
    throw new TypeError("MCP requires trusted binding, catalog and invocation policies.");
  const canonical = selection.createAdapter({
    canList: () => true,
    binding: () => {
      throw new Error("Metadata adapter cannot invoke.");
    },
    maxOutputBytes: maxCatalog,
  });
  const [catalog, agentTools] = await Promise.all([
    canonical.catalog(),
    createAgentTools(canonical),
  ]);
  const tools = agentTools.map((item, index): Tool => {
    const operation = catalog[index]!;
    const tool: Tool = {
      name: `operation_${index}`,
      title: `${operation.pluginId}.${operation.method}`,
      description: [
        item.description,
        ...(operation.outputDescription ? [`Output: ${operation.outputDescription}`] : []),
        "Cancellation signals reach host bindings. Only cooperative services interrupt; cancellation does not roll back effects.",
      ].join("\n"),
      inputSchema: item.inputSchema as Tool["inputSchema"],
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
    async listTools(request: McpRequestContext<I>): Promise<{ tools: Tool[] }> {
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
          // Return a fresh finite copy, preventing callers mutating subsequent discovery.
          return JSON.parse(boundedJson({ tools: visible }, maxCatalog)) as { tools: Tool[] };
        });
      } catch {
        throw new McpError(ErrorCode.InternalError, "Tool discovery failed or was interrupted.");
      }
    },
    async callTool(
      name: string,
      input: unknown,
      request: McpRequestContext<I>,
    ): Promise<CallToolResult> {
      const match = /^operation_(0|[1-9][0-9]*)$/.exec(name);
      if (!match || !catalog[Number(match[1])])
        throw new McpError(ErrorCode.InvalidParams, "Unknown tool.");
      if (closed) return failure("adapter-closed");
      if (pending.size >= maxConcurrent) return failure("adapter-busy");
      if (request.signal.aborted) return failure("request-cancelled");
      try {
        boundedJson(input, maxInput);
      } catch (error) {
        return failure(
          error instanceof EngineError && error.diagnostic.code === "output-too-large"
            ? "input-too-large"
            : "invalid-input",
        );
      }
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
                signal: binding.signal
                  ? AbortSignal.any([context.signal, binding.signal])
                  : context.signal,
              };
            },
          });
          const result = await scoped.invokeEntry(catalog[Number(match[1])]!.key, input);
          context.signal.throwIfAborted();
          return result;
        });
        return { content: [{ type: "text", text: boundedJson(value, maxOutput) }] };
      } catch (error) {
        return failure(error instanceof EngineError ? error.diagnostic.code : "invocation-failed");
      }
    },
    close(): Promise<void> {
      if (closePromise) return closePromise;
      closed = true;
      for (const controller of controllers) controller.abort();
      closePromise = Promise.allSettled(pending).then(() => {
        selection.close();
      });
      return closePromise;
    },
  });
}
