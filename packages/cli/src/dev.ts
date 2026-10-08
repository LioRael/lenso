import { createDevSupervisor, type DevSupervisor } from "@lenso/engine";
import { createDevPresentation, type DevPresentation } from "./dev-presentation";
import { diagnostic } from "./diagnostics";

export type { DevReadyMessage } from "@lenso/engine/dev-ready";

interface DevOptions {
  root: string;
  entry?: string;
  presentation?: DevPresentation;
}

/** Terminal presentation and signal policy adapt the Engine-owned supervisor. */
export async function dev(options: DevOptions): Promise<void> {
  const presentation = options.presentation ?? createDevPresentation({ project: options.root });
  let supervisor: DevSupervisor | undefined;
  let stopped = false;
  const onSignal = () => {
    stopped = true;
    void supervisor?.close().catch((error) => {
      console.error("[lenso]", diagnostic(error));
      presentation.failed();
    });
  };
  process.on("SIGINT", onSignal);
  process.on("SIGTERM", onSignal);
  try {
    supervisor = await createDevSupervisor({
      root: options.root,
      entry: options.entry,
      onEvent(event) {
        switch (event.type) {
          case "starting":
            presentation.starting();
            break;
          case "ready":
            presentation.ready(event);
            break;
          case "failed":
            console.error("[lenso]", event.diagnostic);
            presentation.failed();
            break;
          case "exited":
            console.error(
              `[lenso] Development process exited (${event.code}). Edit source to restart.`,
            );
            presentation.failed();
            break;
        }
      },
    });
    if (stopped) await supervisor.close();
    await supervisor.done;
  } finally {
    process.off("SIGINT", onSignal);
    process.off("SIGTERM", onSignal);
    await supervisor?.close();
  }
}
