import { reportFailure } from "./config";
import { openResources } from "./resources";
import { createLogger } from "@lenso/log";

async function main() {
  let requested = false;
  let requestShutdown!: () => void;
  const shutdownRequested = new Promise<void>((resolve) => {
    requestShutdown = resolve;
  });
  const onSignal = () => {
    if (requested) return;
    requested = true;
    console.error(
      "Shutdown requested: stop claiming, signal active handlers, wait for their completion.",
    );
    requestShutdown();
  };
  process.on("SIGINT", onSignal);
  process.on("SIGTERM", onSignal);
  try {
    const resources = await openResources(undefined, { logger: createLogger() });
    let worker;
    try {
      worker = await resources.queue.startWorker({ concurrency: 2 });
    } catch (error) {
      await resources.close();
      throw error;
    }
    console.error("Report worker ready.");
    try {
      await Promise.race([shutdownRequested, worker.done]);
    } finally {
      // stop() waits for every real handler even when the worker's done promise rejected.
      try {
        await worker.stop({ abort: true });
      } finally {
        await resources.close();
      }
    }
    console.error("Report worker drained and connections closed.");
  } finally {
    process.off("SIGINT", onSignal);
    process.off("SIGTERM", onSignal);
  }
}

if (import.meta.main) {
  await main().catch(() => reportFailure("Worker"));
}
