import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import {
  createDevSupervisor,
  createWorkspaceSupervisor,
  type DevSupervisor,
  type WorkspaceDefinition,
} from "@lenso/engine";
import { createDevPresentation, type DevPresentation } from "./dev-presentation";
import { diagnostic } from "./diagnostics";

interface DevOptions {
  root: string;
  config?: string;
  entry?: string;
  connectionFile?: string;
  workspace?: string;
  workspaceRoot?: string;
  presentation?: DevPresentation;
}

/** Terminal presentation and signal policy adapt the Engine-owned supervisor. */
export async function dev(options: DevOptions): Promise<void> {
  const workspacePath = options.workspace
    ? resolve(options.workspaceRoot ?? process.cwd(), options.workspace)
    : undefined;
  const workspaceRoot = workspacePath
    ? options.workspaceRoot
      ? resolve(options.workspaceRoot)
      : dirname(workspacePath)
    : undefined;
  const presentation =
    options.presentation ?? createDevPresentation({ project: workspaceRoot ?? options.root });
  let supervisor: DevSupervisor | undefined;
  const controller = new AbortController();
  let stopped = false;
  const onSignal = () => {
    stopped = true;
    controller.abort();
    void supervisor?.close().catch((error) => {
      console.error("[lenso]", diagnostic(error));
      presentation.failed();
    });
  };
  process.on("SIGINT", onSignal);
  process.on("SIGTERM", onSignal);
  try {
    if (workspacePath) {
      const workspace = (await import(pathToFileURL(workspacePath).href))
        .default as WorkspaceDefinition;
      supervisor = await createWorkspaceSupervisor({
        workspace,
        root: workspaceRoot,
        signal: controller.signal,
        onEvent(event) {
          console.error("[lenso:workspace]", {
            ...event,
            error: event.error === undefined ? undefined : diagnostic(event.error),
          });
          if (event.status === "ready") presentation.ready({ urls: event.urls });
          else if (event.status === "failed") {
            if (event.retained)
              console.error(
                `[lenso] retained ${event.id} instance=${event.retained.instance} version=${event.retained.version}`,
              );
            presentation.failed();
          } else if (event.status === "starting" || event.status === "building")
            presentation.starting();
        },
      });
      if (stopped) await supervisor.close();
      await supervisor.done;
      return;
    }
    supervisor = await createDevSupervisor({
      root: options.root,
      config: options.config,
      entry: options.entry,
      connectionFile: options.connectionFile,
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
