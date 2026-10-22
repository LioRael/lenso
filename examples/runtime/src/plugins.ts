import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { createRuntimePlugin, type RuntimeService, type RuntimeHandlers } from "@lenso/runtime";
import type { CallOptions } from "@lenso/runtime-sdk";
import type { LogsClient, AggregateInput } from "../.lenso/runtime/logs";
import { v1Contract as logsContract } from "./runtime-contract";
export { logsInput } from "./contracts";

export const runtime: Plugin<RuntimeService<Promise<LogsClient>>> = createRuntimePlugin({
  id: "runtime",
  connection: {
    namespace: "demo",
    service: "frontend",
    instance: process.env.LENSO_WORKSPACE_INSTANCE,
    port: Number(process.env.LENSO_RUNTIME_PORT ?? 7331),
    contract: logsContract,
  },
  clients: async (host) => {
    const { LogsClient } = await import("../.lenso/runtime/logs");
    return new LogsClient(host, { namespace: "demo", service: "batch" });
  },
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
    console.error(JSON.stringify({ event: "plugin_initialized", plugin: "local", count: 1 }));
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
  async setup(context) {
    console.error(JSON.stringify({ event: "plugin_initialized", plugin: "logs", count: 1 }));
    const batch = await context.get(runtime).clients;
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
