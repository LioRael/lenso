import { watch, type FSWatcher } from "node:fs";
import { resolve, join } from "node:path";
import { createDevPresentation, type DevPresentation, type DevReady } from "./dev-presentation";

interface DevOptions {
  root: string;
  entry?: string;
  cliPath: string;
  presentation?: DevPresentation;
}

/** The entry sends this over Bun IPC after its app and listeners have started. */
export interface DevReadyMessage extends DevReady {
  readonly type: "lenso:dev-ready";
  readonly urls?: readonly string[];
}

function isDevReadyMessage(message: unknown): message is DevReadyMessage {
  if (!message || typeof message !== "object" || !("type" in message)) return false;
  if (message.type !== "lenso:dev-ready") return false;
  for (const key of ["urls", "capabilities"] as const) {
    if (key in message) {
      const values: unknown = Reflect.get(message, key);
      if (!Array.isArray(values) || !values.every((value) => typeof value === "string"))
        return false;
    }
  }
  return true;
}

/** Restarts a fresh Bun process after stopping the previous process and its listeners. */
export async function dev(options: DevOptions): Promise<void> {
  const root = resolve(options.root);
  const entry = resolve(root, options.entry ?? "src/server.ts");
  if (!(await Bun.file(entry).exists())) throw new Error(`Development entry missing: ${entry}`);
  const presentation = options.presentation ?? createDevPresentation({ project: root });
  let child: ReturnType<typeof Bun.spawn> | undefined;
  let generator: ReturnType<typeof Bun.spawn> | undefined;
  let closed = false;
  let queued = false;
  let restarting = false;
  let debounce: ReturnType<typeof setTimeout> | undefined;
  let finish!: () => void;
  const done = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const watchers: FSWatcher[] = [];

  async function stopProcess(previous: ReturnType<typeof Bun.spawn> | undefined) {
    if (!previous) return;
    previous.kill("SIGTERM");
    const timeout = setTimeout(() => previous.kill("SIGKILL"), 5000);
    try {
      await previous.exited;
    } finally {
      clearTimeout(timeout);
    }
  }

  async function stopChild() {
    const previous = child;
    child = undefined;
    await stopProcess(previous);
  }

  async function restart() {
    queued = true;
    if (restarting || closed) return;
    restarting = true;
    try {
      while (queued && !closed) {
        queued = false;
        await stopChild();
        if (closed) break;
        presentation.starting();
        // Fresh generation also invalidates imports of the config's dependencies.
        const generated = Bun.spawn(
          [process.execPath, options.cliPath, "generate", "--root", root],
          // Generation's command result is redundant with the dev status; keep diagnostics.
          { cwd: root, stdout: "ignore", stderr: "inherit" },
        );
        generator = generated;
        const generationExit = await generated.exited;
        if (generator === generated) generator = undefined;
        if (closed) break;
        if (generationExit !== 0) {
          console.error("[lenso] Generation failed. Fix the source to restart.");
          presentation.failed();
          continue;
        }
        if (closed) break;
        child = Bun.spawn([process.execPath, entry], {
          cwd: root,
          stdout: "inherit",
          stderr: "inherit",
          ipc(message, subprocess) {
            if (!closed && child === subprocess && isDevReadyMessage(message))
              presentation.ready(message);
          },
        });
        const launched = child;
        void launched.exited.then((code) => {
          if (child === launched && !closed) {
            child = undefined;
            console.error(`[lenso] Development process exited (${code}). Edit source to restart.`);
            presentation.failed();
          }
        });
      }
    } finally {
      restarting = false;
    }
  }

  function changed() {
    if (closed) return;
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(() => {
      void restart().catch((error) => {
        console.error("[lenso]", error);
        presentation.failed();
      });
    }, 100);
  }

  async function close() {
    if (closed) return;
    closed = true;
    if (debounce) clearTimeout(debounce);
    for (const watcher of watchers) watcher.close();
    const generating = generator;
    generator = undefined;
    await stopProcess(generating);
    await stopChild();
    finish();
  }
  const onSignal = () => {
    void close();
  };
  process.on("SIGINT", onSignal);
  process.on("SIGTERM", onSignal);
  try {
    watchers.push(watch(join(root, "src"), { recursive: true }, changed));
    watchers.push(
      watch(root, (_event, filename) => {
        if (filename?.toString() === "lenso.config.ts") changed();
      }),
    );
    await restart();
    await done;
  } finally {
    process.off("SIGINT", onSignal);
    process.off("SIGTERM", onSignal);
    await close();
  }
}
