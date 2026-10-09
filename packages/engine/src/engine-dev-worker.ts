import { createEngineSession } from "./engine";
import { resolve } from "node:path";
import { EngineError, diagnostic, environmentSecrets, redact } from "./diagnostics";

// Trusted build logs belong on stderr; readiness comes only from application IPC.
for (const key of ["log", "info", "debug"] as const) console[key] = console.error.bind(console);
const root = process.argv[2]!;
const overrides: { config?: string; entry?: string } = JSON.parse(process.argv[3] ?? "{}");
const engine = createEngineSession({ root, config: overrides.config }, "dev");
let closing: Promise<void> | undefined;
let queue = Promise.resolve();
let readyFailure: { error: unknown } | undefined;
function send(message: unknown) {
  process.send?.(redact(message, environmentSecrets()));
}
async function close(failure?: { error: unknown }): Promise<void> {
  return (closing ??= (async () => {
    let error = failure === undefined ? undefined : diagnostic(failure.error);
    try {
      await engine.session.close();
    } catch (cleanup) {
      error =
        failure === undefined
          ? diagnostic(cleanup)
          : diagnostic(
              new EngineError({
                code: "engine-and-cleanup-failed",
                phase: "engine-cleanup",
                message: "Engine execution and cleanup failed.",
                causes: [diagnostic(failure.error), diagnostic(cleanup)],
              }),
            );
    }
    send(error === undefined ? { type: "closed" } : { type: "failed", error });
    process.off("message", onMessage);
    process.off("SIGINT", onSignal);
    process.off("SIGTERM", onSignal);
    process.off("disconnect", onSignal);
    process.disconnect?.();
  })());
}
function onSignal() {
  queue = queue.then(() => close(readyFailure));
}
function onMessage(message: unknown) {
  if (!message || typeof message !== "object" || !("type" in message)) return;
  if (message.type === "close") onSignal();
  if (message.type === "ready")
    queue = queue.then(async () => {
      if (closing || readyFailure !== undefined) return;
      try {
        await engine.session.dev("ready");
        send({ type: "ack", id: "id" in message ? message.id : null });
      } catch (cause) {
        readyFailure = { error: cause };
        // The parent must stop the runtime before asking us to clean up.
        send({ type: "failed", error: diagnostic(cause) });
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
    const snapshot = engine.session.snapshot();
    send({
      type: "prepared",
      watchFiles: snapshot.watchFiles,
      entry: resolve(root, overrides.entry ?? snapshot.convention.entry ?? "src/server.ts"),
    });
  } catch (cause) {
    await close({ error: cause });
  }
});
await queue;
