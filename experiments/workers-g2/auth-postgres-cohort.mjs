// Wrap an Auth-owned temporary cohort without adding a path dependency to this
// repository. The supplied command must build/run the Auth candidate itself and
// emit one source-backed result line; this wrapper only verifies provenance and
// converts it to the target manifest's credential-free receipt shape.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { execFileSync, spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const contract = JSON.parse(
  readFileSync(resolve(root, "auth-postgres-cohort.contract.json")),
);

const separator = process.argv.indexOf("--");
const argumentsBeforeCommand = process.argv.slice(2, separator < 0 ? undefined : separator);
const command = separator < 0 ? [] : process.argv.slice(separator + 1);
const options = {};
for (let index = 0; index < argumentsBeforeCommand.length; index += 2) {
  const name = argumentsBeforeCommand[index];
  const value = argumentsBeforeCommand[index + 1];
  if (!['--auth-source', '--output'].includes(name) || !value || name in options)
    throw Error("Usage: node auth-postgres-cohort.mjs --auth-source AUTH_SOURCE --output RECEIPT -- COMMAND [ARG ...]");
  options[name] = value;
}
if (!options['--auth-source'] || !options['--output'] || !command.length)
  throw Error("Auth source, output and a cohort command are required");

const source = resolve(options['--auth-source']);
const sourceFiles = [
  "crates/lenso-auth-oauth-flow-plugin/src/lib.rs",
  "crates/lenso-auth-oauth-flow-plugin/src/postgres_transport.rs",
].map((relative) => resolve(source, relative));
for (const file of sourceFiles)
  if (!existsSync(file)) throw Error(`Auth source is missing ${file.slice(source.length + 1)}`);
const hash = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
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
  // A non-Git input is still recorded as an explicit source snapshot.
}
const sourceSnapshot = {
  revision,
  dirty,
  files: Object.fromEntries(
    sourceFiles.map((file) => [file.slice(source.length + 1), hash(file)]),
  ),
};
const result = spawnSync(command[0], command.slice(1), {
  cwd: source,
  encoding: "utf8",
  maxBuffer: 16 * 1024 * 1024,
  env: {
    ...process.env,
    LENSO_AUTH_POSTGRES_COHORT_SOURCE_REVISION: revision,
  },
});
if (result.error) throw result.error;
// Do not relay an Auth candidate's arbitrary output: this wrapper is also the
// boundary that keeps secrets and private transport details out of receipts.
if (result.status !== 0) throw Error(`Auth cohort command failed with exit ${result.status}`);
const line = (result.stdout || "")
  .split("\n")
  .find((entry) => entry.startsWith(contract.stdoutEvidence.prefix));
if (!line) throw Error("Auth cohort command did not emit the required evidence line");
const evidence = JSON.parse(line.slice(contract.stdoutEvidence.prefix.length));
const expectedCases = new Set(contract.stdoutEvidence.requiredCases);
const passed = new Set(
  evidence.cases?.filter((entry) => entry.passed).map((entry) => entry.name),
);
if (
  evidence.schema !== contract.stdoutEvidence.schema ||
  evidence.environment !== contract.stdoutEvidence.environment ||
  evidence.passed !== true ||
  [...expectedCases].some((name) => !passed.has(name))
)
  throw Error("Auth cohort lacks a passing local-workerd create/consume/revoke/factory-secrecy result");
const receipt = {
  schema: contract.output.receiptSchema,
  case: contract.output.case,
  environment: contract.output.environment,
  passed: true,
  source: {
    repository: "LioRael/lenso-auth-plugin",
    revision,
  },
  observations: {
    cohortSchema: contract.schema,
    cases: [...expectedCases],
    sourceSnapshot,
  },
};
const output = resolve(options['--output']);
mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, JSON.stringify(receipt, null, 2) + "\n");
console.log(JSON.stringify({ passed: true, output, source: receipt.source }));
