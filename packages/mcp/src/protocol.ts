import type { Server } from "@modelcontextprotocol/sdk/server/index.js";
import type { RequestHandlerExtra } from "@modelcontextprotocol/sdk/shared/protocol.js";
import {
  CallToolRequestSchema,
  CancelledNotificationSchema,
  ErrorCode,
  ListToolsRequestSchema,
  McpError,
  type ServerNotification,
  type ServerRequest,
} from "@modelcontextprotocol/sdk/types.js";
import type { createMcpAdapter, McpRequestContext } from "./adapter";

type RequestExtra = RequestHandlerExtra<ServerRequest, ServerNotification>;

/** Keep SDK negotiation/framing, while entries own finite invocation cancellation. */
export function bindMcpServer<I>(
  server: Server,
  adapter: Awaited<ReturnType<typeof createMcpAdapter<I>>>,
  evidence: (extra: RequestExtra) => Omit<McpRequestContext<I>, "signal">,
): { prepare(requestId: string | number): () => void; finish(requestId: string | number): void } {
  const cancellations = new Map<string | number, AbortController>();
  const prepare = (requestId: string | number) => {
    const controller = new AbortController();
    cancellations.set(requestId, controller);
    return () => {
      if (cancellations.get(requestId) === controller) cancellations.delete(requestId);
    };
  };
  const dispatch = async <T>(
    extra: RequestExtra,
    action: (context: McpRequestContext<I>) => Promise<T>,
  ): Promise<T> => {
    const controller = cancellations.get(extra.requestId) ?? new AbortController();
    cancellations.set(extra.requestId, controller);
    try {
      const request = evidence(extra);
      return await action({
        ...request,
        signal: AbortSignal.any([extra.signal, controller.signal]),
      });
    } finally {
      cancellations.delete(extra.requestId);
    }
  };
  // SDK 1.32.1 ignores valid IDs 0/"" and suppresses canceled responses,
  // retaining HTTP correlations. Our finite safe reply releases SDK state.
  server.setNotificationHandler(CancelledNotificationSchema, (notification) => {
    if (notification.params.requestId !== undefined)
      cancellations.get(notification.params.requestId)?.abort();
  });
  server.setRequestHandler(ListToolsRequestSchema, (_request, extra) =>
    dispatch(extra, (context) => adapter.listTools(context)),
  );
  server.setRequestHandler(CallToolRequestSchema, (request, extra) => {
    if (request.params.task)
      throw new McpError(ErrorCode.InvalidParams, "Task-augmented calls are not supported.");
    return dispatch(extra, (context) =>
      adapter.callTool(request.params.name, request.params.arguments ?? {}, context),
    );
  });
  return {
    prepare,
    finish: (requestId) => {
      cancellations.delete(requestId);
    },
  };
}
