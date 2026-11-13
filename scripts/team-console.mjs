import { preview } from "vite";
import { spawn } from "node:child_process";
import { mkdir, readFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { proxyFor } from "../typescript/examples/team-desk-console/vite.config.ts";

const root = resolve(import.meta.dirname, "..");
const directory = resolve(root, process.env.TEAM_CONSOLE_DATA ?? ".local/team-console");
await mkdir(directory, { recursive: true });
const identities = await readFile(join(root, "examples/team-desk/identities.example.json"), "utf8");
const children = [];
let host;
let closing = false;
async function stop() {
  if (closing) return;
  closing = true;
  const stops = children.map((child) => new Promise((resolve) => {
    if (child.exitCode !== null) return resolve();
    child.once("exit", resolve); child.kill("SIGTERM");
    const timer = setTimeout(() => child.kill("SIGKILL"), 3000); timer.unref(); child.once("exit", () => clearTimeout(timer));
  }));
  if (host) stops.push(new Promise((resolve) => host.httpServer.close(resolve)));
  await Promise.all(stops);
}
async function start(command, args, filename) {
  const child = spawn(command, args, { cwd: root, env: { ...process.env, PORT: "0", LENSO_DB: join(directory, filename), LENSO_IDENTITIES: identities }, stdio: ["ignore", "pipe", "inherit"] });
  children.push(child);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("Backend startup timed out")), 15_000);
    child.once("error", (error) => { clearTimeout(timer); reject(error); });
    child.once("exit", (code) => { clearTimeout(timer); if (!closing) { reject(new Error(`Backend exited with ${code}`)); void stop(); } });
    let output = "";
    child.stdout.on("data", (chunk) => {
      output += chunk;
      const port = output.match(/port: (\d+)/)?.[1] ?? output.match(/127\.0\.0\.1:(\d+)/)?.[1];
      if (port) { clearTimeout(timer); resolve(`http://127.0.0.1:${port}`); }
    });
  });
}
try {
  const typescript = await start(process.execPath, [join(root, "typescript/examples/team-desk/dist/server.js")], "typescript.sqlite");
  const rust = await start(join(root, "rust/target/debug/lenso-team-desk"), [], "rust.sqlite");
  host = await preview({ root: join(root, "typescript/examples/team-desk-console"), configFile: false, preview: { host: "127.0.0.1", port: Number(process.env.TEAM_CONSOLE_PORT ?? 5173), strictPort: true, proxy: proxyFor({ typescript, rust }) }, logLevel: "silent" });
  console.log(`Team Desk Console: http://127.0.0.1:${host.httpServer.address().port}`);
  console.log("Local demo identities only. TypeScript and Rust use separate persistent databases.");
  for (const signal of ["SIGINT", "SIGTERM"]) process.once(signal, () => { void stop(); });
} catch (error) { console.error(error); process.exitCode = 1; await stop(); }
