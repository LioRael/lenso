import { Console } from "node:console";
import { resolve } from "node:path";
import { Writable } from "node:stream";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ErrorCode,
  ListToolsRequestSchema,
  McpError,
  ToolSchema,
  type CallToolResult,
  type Tool,
} from "@modelcontextprotocol/sdk/types.js";
import { call, inspect, CliError, diagnostic } from "lenso-cli";
import { environmentSecrets, redact, stableJson } from "@lenso/engine/diagnostics";

export interface StdioOptions {
  /** Trusted launch configuration, never supplied by an MCP tool. */
  root: string;
  allow: readonly { pluginId: string; method: string }[];
  maxInputBytes?: number;
  maxOutputBytes?: number;
  maxFrameBytes?: number;
}

let serving = false;

function failure(
  code: string,
  phase: "arguments" | "discovery" | "input" | "invoke" | "output",
  message: string,
) {
  return new CliError({ code, phase, message });
}

function limit(value: number | undefined, fallback: number): number {
  const result = value ?? fallback;
  if (!Number.isSafeInteger(result) || result < 1)
    throw failure("invalid-arguments", "arguments", "Limits must be positive safe integers.");
  return result;
}

function result(data: unknown, maxBytes: number): CallToolResult {
  stableJson(data);
  const text = stableJson(redact(data, environmentSecrets()));
  if (Buffer.byteLength(text) > maxBytes)
    throw failure("output-too-large", "output", "Operation output exceeds the host limit.");
  return { content: [{ type: "text", text }] };
}

const oversizedDiagnostic = stableJson({
  code: "output-too-large",
  phase: "output",
  message: "Diagnostic exceeds the host limit.",
});

function errorResult(error: unknown, maxBytes: number): CallToolResult {
  let text: string;
  try {
    const detail = diagnostic(error);
    text = stableJson(redact(detail, environmentSecrets()));
    if (Buffer.byteLength(text) > maxBytes) {
      text = stableJson(
        redact(
          {
            code: detail.code,
            phase: detail.phase,
            message: "Diagnostic truncated to the host output limit.",
            truncated: true,
          },
          environmentSecrets(),
        ),
      );
    }
  } catch {
    text = stableJson({
      code: "serialization-failed",
      phase: "output",
      message: "Diagnostic is not JSON data.",
    });
  }
  if (Buffer.byteLength(text) > maxBytes) text = oversizedDiagnostic;
  return { isError: true, content: [{ type: "text", text }] };
}

/**
 * Owns a dedicated stdio process. Tools translate only to CLI call; no app or
 * service lifecycle is implemented here. Close drains the admitted call.
 */
export async function serveStdio(options: StdioOptions): Promise<{ close(): Promise<void> }> {
  if (serving)
    throw failure("invalid-arguments", "arguments", "Only one stdio adapter may own this process.");
  const root = resolve(options.root);
  const maxInput = limit(options.maxInputBytes, 256 * 1024);
  const maxOutput = limit(options.maxOutputBytes, 1024 * 1024);
  if (maxOutput < Buffer.byteLength(oversizedDiagnostic))
    throw failure("invalid-arguments", "arguments", "Output limit must fit a bounded diagnostic.");
  const maxFrame = limit(options.maxFrameBytes, 1024 * 1024);
  serving = true;
  const originalConsole = globalThis.console;
  const logs = new Writable({
    write(chunk, _encoding, done) {
      process.stderr.write(String(redact(chunk.toString(), environmentSecrets())), done);
    },
  });
  globalThis.console = Object.assign(new Console({ stdout: logs, stderr: logs }), {
    write(...values: Array<string | ArrayBufferView | ArrayBuffer>): number {
      const text = values
        .map((value) =>
          typeof value === "string"
            ? value
            : Buffer.from(
                ArrayBuffer.isView(value)
                  ? new Uint8Array(value.buffer, value.byteOffset, value.byteLength)
                  : new Uint8Array(value),
              ).toString(),
        )
        .join("");
      logs.write(text);
      return Buffer.byteLength(text);
    },
  });
  for (const level of [
    "log",
    "info",
    "warn",
    "error",
    "debug",
    "dir",
    "dirxml",
    "table",
    "trace",
  ] as const) {
    globalThis.console[level] = (...values: unknown[]) => {
      const safe = values.map((value) =>
        value instanceof Error
          ? "[Application error text omitted]"
          : redact(value, environmentSecrets()),
      );
      logs.write(
        `${safe.map((value) => (typeof value === "string" ? value : JSON.stringify(value))).join(" ")}\n`,
      );
    };
  }
  globalThis.console.assert = (condition, ...values) => {
    if (!condition) globalThis.console.error("Assertion failed:", ...values);
  };
  let server: Server | undefined;
  let active: Promise<CallToolResult> | undefined;
  let closing = false;
  let closePromise: Promise<void> | undefined;
  const restore = () => {
    globalThis.console = originalConsole;
    serving = false;
  };
  const close = (): Promise<void> => {
    if (closePromise) return closePromise;
    closing = true;
    closePromise = (async () => {
      try {
        await active;
        await server?.close();
      } finally {
        process.stdin.off("end", onEnd);
        process.off("SIGINT", onEnd);
        process.off("SIGTERM", onEnd);
        restore();
      }
    })();
    return closePromise;
  };
  const onEnd = () => {
    void close().catch(() => {
      process.exitCode = 1;
    });
  };
  try {
    const inspection = await inspect(root);
    const tools: Tool[] = [];
    const bindings = new Map<string, { pluginId: string; method: string }>();
    const seen = new Set<string>();
    for (const allowed of options.allow) {
      const key = stableJson([allowed.pluginId, allowed.method]);
      if (seen.has(key))
        throw failure("duplicate-operation", "discovery", "Duplicate MCP allowlist entry.");
      seen.add(key);
      const operation = inspection.operations.find(
        (item) => item.pluginId === allowed.pluginId && item.method === allowed.method,
      );
      if (!operation)
        throw failure("unknown-operation", "discovery", "MCP allowlist operation is not declared.");
      const schema = operation.inputSchema;
      if (!schema || schema.type !== "object")
        throw failure(
          "unsupported-input-schema",
          "discovery",
          "MCP tools require a convertible object input schema.",
        );
      stableJson(schema);
      // An index is protocol-safe and cannot collide even when IDs contain punctuation.
      const name = `operation_${tools.length}`;
      const tool: Tool = {
        name,
        title: `${operation.pluginId}.${operation.method}`,
        description: [
          operation.description,
          ...(operation.outputDescription ? [`Output: ${operation.outputDescription}`] : []),
          "Cancellation is request-only: in-flight work and cleanup are awaited; no rollback or automatic retry.",
        ].join("\n"),
        inputSchema: schema as Tool["inputSchema"],
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
        throw failure(
          "unsupported-input-schema",
          "discovery",
          "Operation schema cannot be represented as an MCP tool.",
        );
      tools.push(tool);
      bindings.set(name, { pluginId: allowed.pluginId, method: allowed.method });
    }
    server = new Server({ name: "lenso", version: "0.1.0" }, { capabilities: { tools: {} } });
    server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools }));
    server.setRequestHandler(CallToolRequestSchema, async (request, extra) => {
      const binding = bindings.get(request.params.name);
      if (!binding)
        throw new McpError(ErrorCode.InvalidParams, "Tool is not allowlisted.", {
          code: "unknown-operation",
        });
      if (request.params.task)
        throw new McpError(ErrorCode.InvalidParams, "Task-augmented calls are not supported.");
      if (closing)
        return errorResult(
          failure("adapter-closed", "invoke", "Adapter is shutting down."),
          maxOutput,
        );
      if (active)
        return errorResult(
          failure(
            "adapter-busy",
            "invoke",
            "One operation is already running; no request was queued.",
          ),
          maxOutput,
        );
      const execute = async (): Promise<CallToolResult> => {
        try {
          if (extra.signal.aborted)
            throw failure("request-cancelled", "invoke", "Request cancelled before invocation.");
          const input = request.params.arguments ?? {};
          if (Buffer.byteLength(stableJson(input)) > maxInput)
            throw failure("input-too-large", "input", "Operation input exceeds the host limit.");
          const data = await call(root, binding.pluginId, binding.method, input);
          if (extra.signal.aborted)
            throw failure(
              "request-cancelled",
              "invoke",
              "Request cancelled; operation may have completed. Cleanup was awaited.",
            );
          return result(data, maxOutput);
        } catch (error) {
          return errorResult(error, maxOutput);
        }
      };
      active = execute();
      try {
        return await active;
      } finally {
        active = undefined;
      }
    });
    server.onerror = () => {
      process.stderr.write("MCP protocol error (details omitted).\n");
    };
    server.onclose = onEnd;
    process.stdin.once("end", onEnd);
    process.on("SIGINT", onEnd);
    process.on("SIGTERM", onEnd);
    await server.connect(
      new StdioServerTransport(process.stdin, process.stdout, { maxBufferSize: maxFrame }),
    );
    return { close };
  } catch (error) {
    await close();
    throw error;
  }
}
