import { mkdir } from "node:fs/promises";
import { spawn } from "node:child_process";
import { resolve } from "node:path";

const runtimeRoot = process.argv[2];
if (!runtimeRoot) throw new Error("Pass the trusted Runtime source root.");
const appRoot = resolve(import.meta.dir, "..");
const output = resolve(appRoot, ".lenso/ts-batch");
await mkdir(output, { recursive: true });
const abort = new AbortController();
const shutdown = () => abort.abort(new Error("TS candidate build cancelled."));
process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);
const child = spawn(
  process.execPath,
  [
    resolve(runtimeRoot, "tools/generate.ts"),
    resolve(runtimeRoot, "examples/contracts/logs.json"),
    output,
    "logs",
  ],
  { cwd: runtimeRoot, stdio: "inherit", detached: process.platform !== "win32" },
);
const kill = (signal: NodeJS.Signals) => {
  try {
    if (process.platform !== "win32" && child.pid) process.kill(-child.pid, signal);
    else child.kill(signal);
  } catch {}
};
const cancel = () => kill("SIGTERM");
abort.signal.addEventListener("abort", cancel, { once: true });
try {
  const code = await new Promise<number | null>((accept, reject) => {
    child.once("error", reject);
    child.once("close", accept);
  });
  abort.signal.throwIfAborted();
  if (code !== 0) throw new Error("TS candidate contract generation failed.");
  const result = await Bun.build({
    entrypoints: [resolve(appRoot, "src/ts-batch.ts")],
    target: "bun",
    packages: "external",
    plugins: [
      {
        name: "candidate-contract",
        setup(build) {
          build.onResolve({ filter: /^\.\/generated\/logs$/ }, () => ({
            path: resolve(output, "logs.ts"),
          }));
        },
      },
    ],
  });
  if (!result.success) throw new AggregateError(result.logs, "TS candidate build failed.");
  abort.signal.throwIfAborted();
  await Bun.write(resolve(output, "server.js"), result.outputs[0]!);
} finally {
  abort.signal.removeEventListener("abort", cancel);
  if (abort.signal.aborted) kill("SIGKILL");
  process.removeListener("SIGINT", shutdown);
  process.removeListener("SIGTERM", shutdown);
}
