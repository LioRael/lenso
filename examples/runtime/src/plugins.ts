import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { createRuntimePlugin, type RuntimeService, type RuntimeHandlers } from "@lenso/runtime";
import type { CallOptions } from "@lenso/runtime-sdk";
import { LogsClient, logsContract, type AggregateInput } from "./generated/logs";
export { logsInput } from "./contracts";

export const runtime: Plugin<RuntimeService<LogsClient>> = createRuntimePlugin({
  id: "runtime",
  connection: {
    namespace: "demo",
    service: "frontend",
    port: Number(process.env.LENSO_RUNTIME_PORT ?? 7331),
    contract: logsContract,
  },
  clients: (host) =>
    new LogsClient(host, {
      namespace: "demo",
      service: "batch",
    }),
  exports(running): RuntimeHandlers {
    const native = running.get(logs);
    return {
      aggregate: async (input, handler) =>
        (
          await native.aggregate(input as AggregateInput, {
            traceId: handler.traceId,
            parentId: handler.callId,
            signal: handler.signal,
          })
        ).batch,
      progress: (input, handler) =>
        native.progress(input as Parameters<LogsClient["progress"]>[0], {
          traceId: handler.traceId,
          parentId: handler.callId,
          signal: handler.signal,
        }),
    };
  },
});

// This business service stays native: no wire contract, registration or generated wrapper.
export const local = definePlugin({
  id: "local",
  setup() {
    let batches = 0;
    return {
      async record() {
        return { batches: ++batches };
      },
    };
  },
});

export const logs = definePlugin({
  id: "logs",
  requires: [runtime, local],
  setup(context) {
    const batch = context.get(runtime).clients;
    const journal = context.get(local);
    return {
      async aggregate(input: AggregateInput, options?: CallOptions) {
        const result = await batch.aggregate(input, options);
        return { batch: result, local: await journal.record() };
      },
      progress(input: Parameters<LogsClient["progress"]>[0], options?: CallOptions) {
        return batch.progress(input, options);
      },
    };
  },
});
