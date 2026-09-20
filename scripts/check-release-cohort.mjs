#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  parseJsonWithUniqueObjectKeys,
  validateReleaseCohort,
  verifyCohortArtifacts,
  verifyCohortWorkspaces,
} from "./release-cohort-lib.mjs";

const scriptDirectory = resolve(fileURLToPath(new URL(".", import.meta.url)));

function usage(message) {
  if (message !== undefined) {
    console.error(message);
  }
  console.error(
    "Usage: node scripts/check-release-cohort.mjs [cohort.json] " +
      "[--workspace owner/repository=/absolute/path] " +
      "[--artifact coordinate=/absolute/path] [--no-verify-receipts]",
  );
  process.exit(1);
}

function assignment(value, option) {
  const equals = value.indexOf("=");
  if (equals <= 0 || equals === value.length - 1) {
    usage(option + " must be NAME=/absolute/path");
  }
  return [value.slice(0, equals), value.slice(equals + 1)];
}

let manifestPath;
let verifyReceipts = true;
const workspaces = new Map();
const artifacts = new Map();
for (let index = 2; index < process.argv.length; index += 1) {
  const argument = process.argv[index];
  if (argument === "--workspace") {
    const value = process.argv[++index];
    if (value === undefined) usage("--workspace needs a value");
    const [repository, path] = assignment(value, "--workspace");
    if (workspaces.has(repository)) usage("duplicate --workspace for " + repository);
    workspaces.set(repository, resolve(path));
  } else if (argument === "--artifact") {
    const value = process.argv[++index];
    if (value === undefined) usage("--artifact needs a value");
    const [coordinate, path] = assignment(value, "--artifact");
    if (artifacts.has(coordinate)) usage("duplicate --artifact for " + coordinate);
    artifacts.set(coordinate, resolve(path));
  } else if (argument === "--no-verify-receipts") {
    verifyReceipts = false;
  } else if (argument.startsWith("-")) {
    usage("unknown option " + argument);
  } else if (manifestPath === undefined) {
    manifestPath = resolve(process.cwd(), argument);
  } else {
    usage("only one cohort manifest may be checked at a time");
  }
}

if (manifestPath === undefined) {
  manifestPath = resolve(
    scriptDirectory,
    "../docs/qualification/cohorts/release-cohort.json",
  );
}

let cohort;
try {
  cohort = parseJsonWithUniqueObjectKeys(readFileSync(manifestPath, "utf8"));
} catch (error) {
  console.error("Cannot parse cohort manifest: " + error.message);
  process.exit(1);
}

const errors = validateReleaseCohort(cohort, {
  manifestDirectory: dirname(manifestPath),
  verifyReceipts,
});
if (workspaces.size > 0) {
  errors.push(...verifyCohortWorkspaces(cohort, workspaces));
}
if (artifacts.size > 0) {
  errors.push(...verifyCohortArtifacts(cohort, artifacts));
}
if (errors.length > 0) {
  console.error(errors.join("\n"));
  process.exit(1);
}

console.log(
  "Release cohort check passed for " + cohort.id +
    " (" + cohort.state + ", " + cohort.source_closure.length + " source repositories, " +
    cohort.artifact_closure.length + " artifacts).",
);
