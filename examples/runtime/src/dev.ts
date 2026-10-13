import { createDevSupervisor } from "@lenso/engine";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";
import { startRuntimeCompanions } from "@lenso/runtime/engine";
import { companionOptions } from "./companions";

if (process.env.LENSO_RUNTIME_EXTERNAL !== "1") {
  throw new Error("Use bun run dev so companion ownership is selected before subprocess startup.");
}

const cleanup: (() => Promise<void>)[] = [];
const abort = new AbortController();
let closing: Promise<void> | undefined;
function close() {
  return (closing ??= (async () => {
    const errors: unknown[] = [];
    for (const dispose of cleanup.reverse()) {
      try {
        await dispose();
      } catch (error) {
        errors.push(error);
      }
    }
    if (errors.length) throw new AggregateError(errors, "Runtime development cleanup failed.");
  })());
}

const shutdown = () => {
  abort.abort(new Error("Development shutdown requested."));
  if (ready)
    void close().catch((error) => {
      console.error(error);
      process.exitCode = 1;
    });
};
let ready = false;
process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);
try {
  const companions = await startRuntimeCompanions(
    { ...companionOptions(), signal: abort.signal },
    (dispose) => cleanup.push(dispose),
  );
  // The supervised Engine workers borrow the outer launcher's already-ready companions.
  const supervisor = await createDevSupervisor({
    root: fileURLToPath(new URL("..", import.meta.url)),
    onEvent(event) {
      console.error(JSON.stringify(event));
    },
  });
  cleanup.push(() => supervisor.close());
  const commands = createInterface({ input: process.stdin });
  cleanup.push(async () => {
    commands.close();
    process.stdin.pause();
  });
  commands.on("line", (command) => {
    if (command.trim() === "restart-batch") {
      void companions.restartService(0).then(
        () => console.error("Rust batch service restarted; kernel and TypeScript graph retained."),
        (error) => {
          if (!abort.signal.aborted) console.error(error);
        },
      );
    }
  });
  ready = true;
  if (abort.signal.aborted) await close();
  console.error(
    "Rebuild Rust separately, then enter restart-batch to reload only the batch executable.",
  );
  await supervisor.done;
} finally {
  await close();
  process.removeListener("SIGINT", shutdown);
  process.removeListener("SIGTERM", shutdown);
}
