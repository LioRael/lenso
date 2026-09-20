// Build and run the target-local suite in locked workerd. The generated Rust/
// Wasm artifact is mandatory; a missing artifact fails rather than falling back
// to a JavaScript imitation.
import { execFileSync, spawnSync } from "node:child_process";
import { gzipSync } from "node:zlib";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
const root = fileURLToPath(new URL(".", import.meta.url));
const bin = root + "node_modules/.pnpm/node_modules/.bin/";
const output = resolve(process.argv[2] || "target-local-workerd.json.gz");

execFileSync(process.execPath, ["prepare-qualification.mjs"], {
  cwd: root,
  stdio: "inherit",
});
execFileSync(
  bin + "esbuild",
  [
    "target-qualification-workerd.mjs",
    "--bundle",
    "--format=esm",
    "--external:*.wasm",
    "--external:node:*",
    "--outfile=.w02/target-qualification-workerd.mjs",
  ],
  { cwd: root, stdio: "inherit" },
);
execFileSync(
  bin + "esbuild",
  [
    "target-ingress-service.mjs",
    "--bundle",
    "--format=esm",
    "--external:*.wasm",
    "--external:node:*",
    "--outfile=.w02/target-ingress-service.mjs",
  ],
  { cwd: root, stdio: "inherit" },
);
const args = [
  "test",
  "-I",
  "node_modules/.pnpm/workerd@1.20260701.1/node_modules",
  "target-qualification.capnp",
];
const startedAt = new Date().toISOString();
const run = spawnSync(bin + "workerd", args, {
  cwd: root,
  encoding: "utf8",
  timeout: 45000,
  maxBuffer: 16 * 1024 * 1024,
});
const log = (run.stdout || "") + (run.stderr || "");
const line = log.split("\n").find((entry) =>
  entry.startsWith("TARGET_QUALIFICATION_EVIDENCE "),
);
const evidence = line
  ? JSON.parse(line.slice("TARGET_QUALIFICATION_EVIDENCE ".length))
  : {
      schema: "workers-target-local-workerd-v1",
      passed: false,
      status: "missing-required-evidence",
      error: String(run.error || log),
      cases: [],
    };
Object.assign(evidence, {
  startedAt,
  command: ["workerd", ...args],
  exitCode: run.status,
  finishedAt: new Date().toISOString(),
  identity: JSON.parse(readFileSync(root + "qualification-identity.json")),
});
mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, gzipSync(JSON.stringify(evidence, null, 2) + "\n", { level: 9 }));
console.log(JSON.stringify(evidence));
if (run.status !== 0 || !evidence.passed) process.exitCode = 1;
