#!/usr/bin/env node

import { resolve } from "node:path";
import { workingTreeSnapshot } from "./release-cohort-lib.mjs";

function usage(message) {
  if (message !== undefined) {
    console.error(message);
  }
  console.error(
    "Usage: node scripts/snapshot-release-cohort.mjs " +
      "--repository owner/repository --workspace /absolute/path " +
      "--coordinate kind:name@version [--coordinate kind:name@version ...]",
  );
  process.exit(1);
}

let repository;
let workspace;
const coordinates = [];
for (let index = 2; index < process.argv.length; index += 1) {
  const argument = process.argv[index];
  const value = process.argv[++index];
  if (value === undefined) usage(argument + " needs a value");
  if (argument === "--repository") {
    repository = value;
  } else if (argument === "--workspace") {
    workspace = resolve(value);
  } else if (argument === "--coordinate") {
    coordinates.push(value);
  } else {
    usage("unknown option " + argument);
  }
}
if (repository === undefined || workspace === undefined || coordinates.length === 0) {
  usage("repository, workspace, and at least one coordinate are required");
}
if (new Set(coordinates).size !== coordinates.length) {
  usage("coordinates must be unique");
}

try {
  const result = workingTreeSnapshot(workspace);
  console.log(
    JSON.stringify(
      {
        repository,
        revision: result.revision,
        snapshot: result.snapshot,
        coordinates,
      },
      null,
      2,
    ),
  );
} catch (error) {
  console.error("Cannot snapshot source workspace: " + error.message);
  process.exit(1);
}
