import { resolve } from "node:path";
import { spawn, type ChildProcess } from "node:child_process";
import { prepareRuntime } from "./prepare-runtime";

const root = resolve(import.meta.dir, "..");
const runtimeRoot = process.env.LENSO_RUNTIME_ROOT;
if (!runtimeRoot || !process.env.LENSO_RUST_ROOT)
  throw new Error("Set LENSO_RUNTIME_ROOT and LENSO_RUST_ROOT to trusted local source checkouts.");

let child: ChildProcess | undefined;
let stopping = false;
const abort = new AbortController();
let timer: ReturnType<typeof setTimeout> | undefined;
const shutdown = () => {
  stopping = true;
  abort.abort(new Error("Workspace startup cancelled."));
  child?.kill("SIGTERM");
  if (child && !timer)
    timer = setTimeout(() => {
      if (process.platform !== "win32" && child?.pid) {
        try {
          process.kill(-child.pid, "SIGKILL");
        } catch {}
      } else child?.kill("SIGKILL");
    }, 40000);
};
process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);
async function run(args: string[], program = process.execPath) {
  if (stopping) throw new Error("Workspace startup cancelled.");
  child = spawn(program, args, {
    cwd: root,
    stdio: "inherit",
    detached: process.platform !== "win32",
  });
  const code = await new Promise<number | null>((accept, reject) => {
    child!.once("error", reject);
    child!.once("close", accept);
  });
  if (stopping && process.platform !== "win32" && child.pid) {
    try {
      process.kill(-child.pid, "SIGKILL");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
    }
  }
  clearTimeout(timer);
  timer = undefined;
  child = undefined;
  if (code) throw new Error(`Workspace command failed: bun ${args.join(" ")}`);
}
try {
  await prepareRuntime(root, {}, { runtimeRoot: resolve(runtimeRoot), signal: abort.signal });
  await run(["install", "--frozen-lockfile"]);
  await run([
    "x",
    "--no-install",
    "turbo",
    "run",
    "build",
    "--filter=@lenso/cli...",
    "--filter=@lenso/runtime...",
    "--filter=@lenso/cache...",
    "--filter=@lenso/log...",
  ]);
  await run(["dev", "--workspace", "examples/runtime/lenso.workspace.ts"], "lenso");
} finally {
  clearTimeout(timer);
  process.removeListener("SIGINT", shutdown);
  process.removeListener("SIGTERM", shutdown);
}
