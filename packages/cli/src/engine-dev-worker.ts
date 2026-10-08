import { createEngineSession } from "./engine";
import { CliError, diagnostic, environmentSecrets, redact } from "./diagnostics";

// This trusted build host has its own lifecycle; only the application IPC can signal Ready.
for (const key of ["log", "info", "debug"] as const) console[key] = console.error.bind(console);
const engine = createEngineSession(process.argv[2]!, "dev");
let closing: Promise<void> | undefined;
let queue = Promise.resolve();
function send(message: unknown) {
  process.send?.(redact(message, environmentSecrets()));
}
async function close(failure?: unknown): Promise<void> {
  return (closing ??= (async () => {
    let error = failure;
    try {
      await engine.session.close();
    } catch (cleanup) {
      error =
        failure === undefined
          ? cleanup
          : new CliError({
              code: "engine-and-cleanup-failed",
              phase: "engine-cleanup",
              message: "Engine execution and cleanup failed.",
              causes: [diagnostic(failure), diagnostic(cleanup)],
            });
    }
    send(error === undefined ? { type: "closed" } : { type: "failed", error: diagnostic(error) });
    process.exitCode = error === undefined ? 0 : 1;
    process.off("message", onMessage);
    process.off("SIGINT", onSignal);
    process.off("SIGTERM", onSignal);
    process.off("disconnect", onSignal);
    process.disconnect?.();
  })());
}
function onSignal() {
  queue = queue.then(() => close());
}
function onMessage(message: unknown) {
  if (!message || typeof message !== "object" || !("type" in message)) return;
  if (message.type === "close") onSignal();
  if (message.type === "ready")
    queue = queue.then(async () => {
      if (closing) return;
      try {
        await engine.session.dev("ready");
        send({ type: "ack", id: "id" in message ? message.id : null });
      } catch (cause) {
        await close(cause);
      }
    });
}
process.on("message", onMessage);
process.on("SIGINT", onSignal);
process.on("SIGTERM", onSignal);
process.on("disconnect", onSignal);
queue = queue.then(async () => {
  try {
    await engine.prepare();
    await engine.session.generate();
    await engine.session.dev("beforeStart");
    send({ type: "prepared", watchFiles: engine.session.snapshot().watchFiles });
  } catch (cause) {
    await close(cause);
  }
});
await queue;
