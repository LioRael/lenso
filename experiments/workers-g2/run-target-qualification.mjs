// Compose local workerd evidence with explicitly supplied external receipts.
// It never deploys, reads credentials, creates platform resources or upgrades a
// target claim. `--require-external` is deliberately opt-in: local CI can prove
// its local boundary while a release cohort can fail closed on missing receipts.
import { execFileSync, spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { gunzipSync } from "node:zlib";
import { dirname, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL(".", import.meta.url));
const manifest = JSON.parse(
  readFileSync(resolve(root, "target-qualification.manifest.json")),
);
const optionNames = new Set([
  "--output",
  "--d1-receipt",
  "--client-disconnect-receipt",
  "--postgres-hyperdrive-receipt",
  "--auth-postgres-receipt",
  "--auth-source",
  "--require-external",
]);
const options = {};
for (let index = 2; index < process.argv.length; index++) {
  const name = process.argv[index];
  if (!optionNames.has(name) || name in options)
    throw Error("Usage: node run-target-qualification.mjs --output REPORT [--auth-source AUTH_SOURCE] [--auth-postgres-receipt PATH] [--d1-receipt PATH] [--client-disconnect-receipt PATH] [--postgres-hyperdrive-receipt PATH] [--require-external]");
  if (name === "--require-external") options[name] = true;
  else {
    const value = process.argv[++index];
    if (!value || value.startsWith("--")) throw Error(`${name} requires a path`);
    options[name] = value;
  }
}
if (!options["--output"])
  throw Error("--output is required so qualification output is never mistaken for checked-in evidence");

const readJson = (path) => {
  const bytes = readFileSync(path);
  return JSON.parse((bytes[0] === 0x1f && bytes[1] === 0x8b ? gunzipSync(bytes) : bytes).toString("utf8"));
};
const sha256 = (path) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");
const run = (script, args) => {
  const result = spawnSync(process.execPath, [script, ...args], {
    cwd: root,
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw Error(`${script} failed with exit ${result.status}`);
};

function requiredCases(evidence, names, label) {
  if (!evidence?.passed) throw Error(`${label} did not pass`);
  const passed = new Set(
    evidence.cases
      ?.filter((entry) => entry.passed)
      .map((entry) => entry.name),
  );
  for (const name of names)
    if (!passed.has(name)) throw Error(`${label} lacks passing case ${name}`);
}

function sourceSnapshot(path) {
  const source = resolve(path);
  const files = [
    "crates/lenso-auth-oauth-flow-plugin/src/lib.rs",
    "crates/lenso-auth-oauth-flow-plugin/src/postgres_transport.rs",
  ].map((relative) => resolve(source, relative));
  for (const file of files)
    if (!existsSync(file)) throw Error(`Auth source is missing ${file.slice(source.length + 1)}`);
  let revision = "unversioned-source-snapshot";
  let dirty = true;
  try {
    revision = execFileSync("git", ["-C", source, "rev-parse", "HEAD"], {
      encoding: "utf8",
    }).trim();
    dirty = Boolean(
      execFileSync("git", ["-C", source, "status", "--porcelain"], {
        encoding: "utf8",
      }).trim(),
    );
  } catch {
    // A source snapshot remains valid input metadata when it is deliberately
    // outside Git, but cannot turn an external receipt into a platform claim.
  }
  return {
    revision,
    dirty,
    files: Object.fromEntries(
      files.map((file) => [file.slice(source.length + 1), sha256(file)]),
    ),
  };
}

const forbiddenReceiptKey = /(?:secret|token|password|credential|connection|binding|private.?url|authorization)/i;
function receiptHasForbiddenKey(value) {
  if (!value || typeof value !== "object") return false;
  return Object.entries(value).some(([key, next]) =>
    forbiddenReceiptKey.test(key) || receiptHasForbiddenKey(next),
  );
}
function receipt(caseDefinition, path) {
  const value = readJson(path);
  if (value.schema !== manifest.receiptSchema.schema)
    throw Error(`${caseDefinition.id}: unsupported receipt schema`);
  if (value.case !== caseDefinition.case || value.environment !== caseDefinition.environment)
    throw Error(`${caseDefinition.id}: receipt case or environment differs from the manifest`);
  if (value.passed !== true || !value.source || typeof value.source.repository !== "string" || typeof value.source.revision !== "string" || !value.observations || typeof value.observations !== "object")
    throw Error(`${caseDefinition.id}: receipt lacks a passing source-backed observation`);
  if (receiptHasForbiddenKey(value))
    throw Error(`${caseDefinition.id}: receipt contains a prohibited sensitive key`);
  return {
    status: "passed-external-receipt",
    source: value.source,
    receiptSha256: sha256(path),
  };
}

const temporary = mkdtempSync(resolve(tmpdir(), "lenso-workers-target-qualification-"));
const w02Path = resolve(temporary, "w02.json.gz");
const localPath = resolve(temporary, "target-local.json.gz");
const startedAt = new Date().toISOString();
run("run-workerd-qualification.mjs", [w02Path]);
run("run-target-local-qualification.mjs", [localPath]);
const w02 = readJson(w02Path);
const local = readJson(localPath);
if (w02.schema !== "w02-local-matrix-v1") throw Error("unexpected W02 evidence schema");
if (local.schema !== "workers-target-local-workerd-v1")
  throw Error("unexpected target-local evidence schema");
requiredCases(
  w02,
  ["held-stream", "held-websocket", "cancellation", "quarantine", "late-reject", "cleanup-reject"],
  "W02 local workerd evidence",
);
requiredCases(
  local,
  [
    "wasm-trap-generation-abandonment-late-cleanup",
    "actual-generated-worker-fetch-body-timeout-and-recovery",
    "workerd-service-host-callback-failure-is-opaque-to-auth",
    "workerd-service-host-callback-timeout-cancels-the-owner-operation",
  ],
  "target local workerd evidence",
);
if (w02.artifact?.wasmSha256 !== local.identity?.wasmSha256)
  throw Error("W02 and target-local evidence use different generated Wasm artifacts");
if (w02.artifact?.sourceSha256 !== local.identity?.sourceSha256)
  throw Error("W02 and target-local evidence use different source identities");

const inputs = {
  "auth-postgres-composition": options["--auth-postgres-receipt"],
  "d1-failure": options["--d1-receipt"],
  "client-disconnect": options["--client-disconnect-receipt"],
  "postgres-hyperdrive-failure": options["--postgres-hyperdrive-receipt"],
};
const external = Object.fromEntries(
  manifest.externalReceipts.map((definition) => [
    definition.id,
    inputs[definition.id]
      ? receipt(definition, resolve(inputs[definition.id]))
      : { status: "external-prerequisite-pending", required: definition.required },
  ]),
);
const pending = Object.entries(external)
  .filter(([, value]) => value.status !== "passed-external-receipt")
  .map(([id]) => id);
const report = {
  schema: "lenso-workers-target-qualification-report-v1",
  manifest: {
    schema: manifest.schema,
    sha256: sha256(resolve(root, "target-qualification.manifest.json")),
  },
  status: pending.length
    ? "local-workerd-passed-external-gates-pending"
    : "external-receipts-present-production-rollout-still-separate",
  environment: "local-workerd",
  startedAt,
  finishedAt: new Date().toISOString(),
  local: {
    status: "passed",
    w02: {
      sourceSha256: w02.artifact.sourceSha256,
      wasmSha256: w02.artifact.wasmSha256,
      cases: w02.cases.filter((entry) => entry.passed).map((entry) => entry.name),
    },
    target: {
      sourceSha256: local.identity.sourceSha256,
      wasmSha256: local.identity.wasmSha256,
      cases: local.cases.filter((entry) => entry.passed).map((entry) => entry.name),
    },
  },
  authSourceSnapshot: options["--auth-source"]
    ? sourceSnapshot(options["--auth-source"])
    : undefined,
  external,
  pendingExternalGates: pending,
  qualificationBoundary: manifest.localWorkerd.doesNotProve,
  productionDeployment: "not-qualified-by-this-harness",
};
const output = resolve(options["--output"]);
mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, JSON.stringify(report, null, 2) + "\n");
console.log(JSON.stringify({ status: report.status, output, pendingExternalGates: pending }));
if (options["--require-external"] && pending.length) process.exitCode = 1;
