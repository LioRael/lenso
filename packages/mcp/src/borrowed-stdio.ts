import { Console } from "node:console";
import { Writable } from "node:stream";
import { finished } from "node:stream/promises";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  ErrorCode,
  McpError,
  isJSONRPCErrorResponse,
  isJSONRPCRequest,
  isJSONRPCResultResponse,
} from "@modelcontextprotocol/sdk/types.js";
import type { Operation } from "@lenso/engine/operations";
import { environmentSecrets, redact } from "@lenso/engine/diagnostics";
import { createMcpAdapter, type McpAdapterOptions } from "./adapter";
import { bindMcpServer } from "./protocol";

export interface BorrowedStdioOptions<I, O extends Operation = Operation> extends McpAdapterOptions<
  I,
  O
> {
  /** Trusted, fixed launch identity, never supplied by tool arguments. */
  readonly identity: I;
  readonly maxFrameBytes?: number;
}

let stdioOwned = false;

/** Shared with other dedicated stdio entries; HTTP must not acquire this latch. */
export function acquireStdioOwnership(): () => void {
  if (stdioOwned) throw new TypeError("Only one stdio adapter may own this process.");
  stdioOwned = true;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    stdioOwned = false;
  };
}

/** Borrows the running application; the host alone owns app.stop(). */
export async function serveBorrowedStdio<I, O extends Operation = Operation>(
  options: BorrowedStdioOptions<I, O>,
): Promise<{ close(): Promise<void> }> {
  const maxFrame = options.maxFrameBytes ?? 1024 * 1024;
  if (!Number.isSafeInteger(maxFrame) || maxFrame < 1) throw new TypeError("Invalid MCP limit.");
  const release = acquireStdioOwnership();
  const originalConsole = globalThis.console;
  let identity: I;
  const logs = new Writable({
    write(chunk, _encoding, done) {
      process.stderr.write(String(redact(chunk.toString(), environmentSecrets())), done);
    },
  });
  let adapter: Awaited<ReturnType<typeof createMcpAdapter<I, O>>> | undefined;
  let server: Server | undefined;
  let transport: StdioServerTransport | undefined;
  let closing = false;
  let closePromise: Promise<void> | undefined;
  const close = (): Promise<void> => {
    if (closePromise) return closePromise;
    closing = true;
    // Defer cleanup so synchronous SDK onclose cannot re-enter before caching.
    closePromise = Promise.resolve().then(async () => {
      const errors: unknown[] = [];
      try {
        try {
          await adapter?.close();
        } catch (error) {
          errors.push(error);
        }
        try {
          await server?.close();
        } catch (error) {
          errors.push(error);
        }
        try {
          await transport?.close();
        } catch (error) {
          errors.push(error);
        }
      } finally {
        process.stdin.off("end", onEnd);
        process.off("SIGINT", onEnd);
        process.off("SIGTERM", onEnd);
        globalThis.console = originalConsole;
        try {
          logs.end();
          await finished(logs);
        } catch (error) {
          errors.push(error);
        }
        release();
      }
      if (errors.length) throw new AggregateError(errors, "MCP close failed.");
    });
    return closePromise;
  };
  const onEnd = () => {
    void close().catch(() => {
      process.exitCode = 1;
    });
  };
  const context = () => {
    if (closing) throw new McpError(ErrorCode.InvalidRequest, "Adapter is shutting down.");
    return { identity, requestId: crypto.randomUUID() };
  };

  try {
    identity = options.identity;
    // Match serveStdio's redaction boundary, including Bun's console.write.
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
    adapter = await createMcpAdapter(options);
    const borrowed = adapter;
    server = new Server({ name: "lenso", version: "0.2.1" }, { capabilities: { tools: {} } });
    const protocol = bindMcpServer(server, borrowed, context);
    server.onerror = () => {
      process.stderr.write("MCP protocol error (details omitted).\n");
    };
    server.onclose = onEnd;
    process.stdin.once("end", onEnd);
    process.on("SIGINT", onEnd);
    process.on("SIGTERM", onEnd);
    transport = new StdioServerTransport(process.stdin, process.stdout, {
      maxBufferSize: maxFrame,
    });
    await server.connect(transport);
    const incoming = transport.onmessage;
    transport.onmessage = (message) => {
      if (
        isJSONRPCRequest(message) &&
        (message.method === "tools/list" || message.method === "tools/call")
      )
        protocol.prepare(message.id);
      incoming?.(message);
    };
    const send = transport.send.bind(transport);
    transport.send = async (message) => {
      try {
        await send(message);
      } finally {
        if (
          (isJSONRPCResultResponse(message) || isJSONRPCErrorResponse(message)) &&
          message.id !== undefined
        )
          protocol.finish(message.id);
      }
    };
    if (process.stdin.readableEnded) onEnd();
    if (closing) {
      await close();
      throw new McpError(ErrorCode.InvalidRequest, "Adapter closed during startup.");
    }
    return Object.freeze({ close });
  } catch (error) {
    try {
      await close();
    } catch (closeError) {
      throw new AggregateError([error, closeError], "MCP startup and close failed.", {
        cause: closeError,
      });
    }
    throw error;
  }
}
