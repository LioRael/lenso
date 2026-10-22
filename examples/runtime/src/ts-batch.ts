import { defineApp } from "@lenso/core";
import { createRuntimePlugin, startRuntimeApp } from "@lenso/runtime";
import { RuntimeError } from "@lenso/runtime-sdk";
import { reportDevReady } from "@lenso/engine/dev-ready";
import { setTimeout as delay } from "node:timers/promises";
import { batch, batchPlugins } from "./batch-app";
import { EmptyBatchError } from "./aggregate";
import { logsContract, type AggregateInput, type ProgressInput } from "../.lenso/runtime/logs";

const transport = createRuntimePlugin({
  id: "runtime",
  connection: {
    namespace: "demo",
    service: process.env.LENSO_RUNTIME_SERVICE ?? "batch",
    instance: process.env.LENSO_RUNTIME_INSTANCE ?? process.env.LENSO_WORKSPACE_INSTANCE,
    port: Number(process.env.LENSO_RUNTIME_PORT ?? 7331),
    contract: logsContract,
    maxConcurrency: 8,
  },
  clients: () => undefined,
  exports(running) {
    const service = running.get(batch);
    return {
      async aggregate(input, context) {
        try {
          return await service.aggregate(input as AggregateInput, context.signal);
        } catch (error) {
          if (error instanceof EmptyBatchError)
            throw new RuntimeError("business", error.message, { reason: error.message });
          throw error;
        }
      },
      async *progress(input, context) {
        const request = input as ProgressInput;
        if (!request.logs.length)
          throw new RuntimeError("business", "At least one log is required", {
            reason: "At least one log is required",
          });
        for (let index = 0; index < request.logs.length; index++) {
          context.signal.throwIfAborted();
          if (request.delay_ms)
            await delay(request.delay_ms, undefined, { signal: context.signal });
          yield { processed: index + 1, total: request.logs.length };
        }
      },
    };
  },
});
const running = await startRuntimeApp(defineApp({ plugins: [...batchPlugins, transport] }));
console.error(
  JSON.stringify({
    event: "service_ready",
    namespace: "demo",
    service: process.env.LENSO_RUNTIME_SERVICE ?? "batch",
    instance: process.env.LENSO_RUNTIME_INSTANCE ?? process.env.LENSO_WORKSPACE_INSTANCE,
    contract_version: logsContract.version,
  }),
);
reportDevReady({ capabilities: ["runtime/logs"] });
let stopping: Promise<void> | undefined;
const close = () => (stopping ??= running.stop());
let finish!: () => void;
const stopped = new Promise<void>((resolve) => {
  finish = resolve;
});
const shutdown = () => {
  void close()
    .finally(finish)
    .catch(() => {});
};
process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);
try {
  await stopped;
} finally {
  process.removeListener("SIGINT", shutdown);
  process.removeListener("SIGTERM", shutdown);
  await close();
}
