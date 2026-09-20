#!/usr/bin/env node

import { readFileSync, readdirSync } from "node:fs";
import { dirname, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import {
  compareCodeUnits,
  isCohortId,
  isDigest,
  isSafeRelativePath,
  parseJsonWithUniqueObjectKeys as parseCohortJson,
  releaseCohortSources,
  sha256,
  validateReleaseCohort,
} from "./release-cohort-lib.mjs";

const scriptDirectory = resolve(fileURLToPath(new URL(".", import.meta.url)));
let ledgerArgument;
let cohortDirectory = resolve(scriptDirectory, "../docs/qualification/cohorts");
for (let index = 2; index < process.argv.length; index += 1) {
  const argument = process.argv[index];
  if (argument === "--cohort-directory") {
    const value = process.argv[++index];
    if (value === undefined) {
      console.error("--cohort-directory needs a path");
      process.exit(1);
    }
    cohortDirectory = resolve(process.cwd(), value);
  } else if (argument.startsWith("-")) {
    console.error("Unknown option " + argument);
    process.exit(1);
  } else if (ledgerArgument === undefined) {
    ledgerArgument = argument;
  } else {
    console.error("Only one ledger path may be checked at a time.");
    process.exit(1);
  }
}
const ledgerPath =
  ledgerArgument === undefined
    ? resolve(scriptDirectory, "../docs/qualification/qualification-status.json")
    : resolve(process.cwd(), ledgerArgument);
const ledgerDirectory = dirname(ledgerPath);
const errors = [];

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isNonEmptyString(value) {
  return typeof value === "string" && value.length > 0;
}

function parseJsonWithUniqueObjectKeys(source) {
  let index = 0;

  function fail(detail) {
    throw new SyntaxError(detail + " at byte " + index);
  }

  function skipWhitespace() {
    while (" \n\r\t".includes(source[index])) {
      index += 1;
    }
  }

  function parseString() {
    const start = index;
    index += 1;
    while (index < source.length) {
      const character = source[index];
      if (character === "\"") {
        index += 1;
        try {
          return JSON.parse(source.slice(start, index));
        } catch {
          fail("invalid JSON string");
        }
      }
      if (character === "\\") {
        index += 1;
        const escape = source[index];
        if (escape === "u") {
          const digits = source.slice(index + 1, index + 5);
          if (!/^[0-9a-fA-F]{4}$/.test(digits)) {
            fail("invalid unicode escape");
          }
          index += 5;
          continue;
        }
        if (!["\"", "\\", "/", "b", "f", "n", "r", "t"].includes(escape)) {
          fail("invalid string escape");
        }
        index += 1;
        continue;
      }
      if (character.codePointAt(0) <= 0x1f) {
        fail("control character in string");
      }
      index += 1;
    }
    fail("unterminated JSON string");
  }

  function parseLiteral(text, value) {
    if (!source.startsWith(text, index)) {
      fail("invalid JSON literal");
    }
    index += text.length;
    return value;
  }

  function parseNumber() {
    const match = /^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/.exec(
      source.slice(index),
    );
    if (match === null) {
      fail("invalid JSON number");
    }
    index += match[0].length;
    return Number(match[0]);
  }

  function parseArray() {
    const values = [];
    index += 1;
    skipWhitespace();
    if (source[index] === "]") {
      index += 1;
      return values;
    }
    while (true) {
      values.push(parseValue());
      skipWhitespace();
      if (source[index] === "]") {
        index += 1;
        return values;
      }
      if (source[index] !== ",") {
        fail("expected comma or closing array");
      }
      index += 1;
      skipWhitespace();
      if (source[index] === "]") {
        fail("trailing comma in array");
      }
    }
  }

  function parseObject() {
    const value = Object.create(null);
    const keys = new Set();
    index += 1;
    skipWhitespace();
    if (source[index] === "}") {
      index += 1;
      return value;
    }
    while (true) {
      if (source[index] !== "\"") {
        fail("object key must be a JSON string");
      }
      const key = parseString();
      if (keys.has(key)) {
        fail("duplicate object key " + JSON.stringify(key));
      }
      keys.add(key);
      skipWhitespace();
      if (source[index] !== ":") {
        fail("expected colon after object key");
      }
      index += 1;
      value[key] = parseValue();
      skipWhitespace();
      if (source[index] === "}") {
        index += 1;
        return value;
      }
      if (source[index] !== ",") {
        fail("expected comma or closing object");
      }
      index += 1;
      skipWhitespace();
      if (source[index] === "}") {
        fail("trailing comma in object");
      }
    }
  }

  function parseValue() {
    skipWhitespace();
    switch (source[index]) {
      case "{":
        return parseObject();
      case "[":
        return parseArray();
      case "\"":
        return parseString();
      case "t":
        return parseLiteral("true", true);
      case "f":
        return parseLiteral("false", false);
      case "n":
        return parseLiteral("null", null);
      default:
        return parseNumber();
    }
  }

  const value = parseValue();
  skipWhitespace();
  if (index !== source.length) {
    fail("unexpected trailing JSON input");
  }
  return value;
}

function canonicalize(value) {
  if (Array.isArray(value)) {
    return value.map(canonicalize);
  }
  if (isObject(value)) {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => compareCodeUnits(left, right))
        .map(([key, item]) => [key, canonicalize(item)]),
    );
  }
  return value;
}

function normalizedSourceRefs(sourceRefs, location) {
  if (!Array.isArray(sourceRefs) || sourceRefs.length === 0) {
    errors.push(location + " must contain source references");
    return null;
  }

  const fingerprints = new Set();
  const normalized = [];
  for (const sourceRef of sourceRefs) {
    if (
      !isObject(sourceRef) ||
      !isNonEmptyString(sourceRef.repository) ||
      !/^[0-9a-f]{40}$/.test(sourceRef.revision) ||
      !Array.isArray(sourceRef.paths) ||
      sourceRef.paths.length === 0 ||
      !sourceRef.paths.every(isNonEmptyString)
    ) {
      errors.push(location + " contains an invalid source reference");
      return null;
    }
    const paths = [...sourceRef.paths].sort(compareCodeUnits);
    if (new Set(paths).size !== paths.length) {
      errors.push(location + " repeats a path inside one source reference");
      return null;
    }
    let cohort;
    if (sourceRef.cohort !== undefined) {
      if (
        !isObject(sourceRef.cohort) ||
        !isCohortId(sourceRef.cohort.id) ||
        !isDigest(sourceRef.cohort.snapshot_digest)
      ) {
        errors.push(location + " contains an invalid cohort reference");
        return null;
      }
      cohort = {
        id: sourceRef.cohort.id,
        snapshot_digest: sourceRef.cohort.snapshot_digest,
      };
    }
    const normalizedRef = {
      repository: sourceRef.repository,
      revision: sourceRef.revision,
      paths,
    };
    if (cohort !== undefined) {
      normalizedRef.cohort = cohort;
    }
    const fingerprint = JSON.stringify(normalizedRef);
    if (fingerprints.has(fingerprint)) {
      errors.push(location + " repeats a source reference");
      return null;
    }
    fingerprints.add(fingerprint);
    normalized.push(normalizedRef);
  }

  return normalized.sort((left, right) =>
    compareCodeUnits(JSON.stringify(left), JSON.stringify(right)),
  );
}

function normalizedInfrastructure(infrastructure, location) {
  if (!Array.isArray(infrastructure) || infrastructure.length === 0) {
    errors.push(location + " must contain at least one infrastructure role");
    return null;
  }

  const roles = new Set();
  const normalized = [];
  for (const entry of infrastructure) {
    if (
      !isObject(entry) ||
      !isNonEmptyString(entry.role) ||
      !isNonEmptyString(entry.implementation)
    ) {
      errors.push(location + " contains an invalid infrastructure role");
      return null;
    }
    if (roles.has(entry.role)) {
      errors.push(location + " repeats infrastructure role " + entry.role);
      return null;
    }
    roles.add(entry.role);
    normalized.push({
      role: entry.role,
      implementation: entry.implementation,
    });
  }

  return normalized.sort((left, right) =>
    left.role === right.role
      ? compareCodeUnits(left.implementation, right.implementation)
      : compareCodeUnits(left.role, right.role),
  );
}

function sourceReferenceKey(reference) {
  return [
    reference.repository,
    reference.revision,
    reference.cohort?.snapshot_digest ?? "",
  ].join("\u0000");
}

function loadCohortIndex(directory) {
  const index = new Map();
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isFile() || !entry.name.endsWith(".json")) {
      continue;
    }
    const path = resolve(directory, entry.name);
    let cohort;
    try {
      cohort = parseCohortJson(readFileSync(path, "utf8"));
    } catch (error) {
      errors.push("Cannot parse cohort " + entry.name + ": " + error.message);
      continue;
    }
    const cohortErrors = validateReleaseCohort(cohort, {
      manifestDirectory: directory,
    });
    if (cohortErrors.length > 0) {
      errors.push(
        ...cohortErrors.map((detail) => "Cohort " + entry.name + ": " + detail),
      );
      continue;
    }
    if (index.has(cohort.id)) {
      errors.push("Cohort " + entry.name + " repeats cohort ID " + cohort.id);
      continue;
    }
    const sources = new Map();
    for (const source of releaseCohortSources(cohort)) {
      sources.set(
        [source.repository, source.revision, source.snapshot_digest].join("\u0000"),
        source,
      );
    }
    index.set(cohort.id, { cohort, sources });
  }
  return index;
}

function validateCohortReference(reference, location, policy, cohortIndex) {
  if (reference.cohort === undefined) {
    return;
  }
  const indexed = cohortIndex.get(reference.cohort.id);
  if (indexed === undefined) {
    errors.push(location + " references missing cohort " + reference.cohort.id);
    return;
  }
  const source = indexed.sources.get(sourceReferenceKey(reference));
  if (source === undefined) {
    errors.push(location + " does not match a source in cohort " + reference.cohort.id);
    return;
  }
  if (policy === "release") {
    if (indexed.cohort.state !== "published" || source.snapshot_kind !== "committed") {
      errors.push(location + " uses a non-published cohort for a Released claim");
    }
  }
  if (policy === "target-or-production") {
    if (
      !["release-ready", "published"].includes(indexed.cohort.state) ||
      source.snapshot_kind !== "committed"
    ) {
      errors.push(
        location + " uses a mutable or non-release-ready cohort for a target or production claim",
      );
    }
  }
}

function checkSourceRefs(sourceRefs, location, policy, cohortIndex) {
  const normalized = normalizedSourceRefs(sourceRefs, location);
  if (normalized === null) {
    return null;
  }
  normalized.forEach((reference, index) =>
    validateCohortReference(reference, location + "[" + index + "]", policy, cohortIndex),
  );
  return normalized;
}

function checkRetainedReceipt(receipt, location) {
  if (
    !isObject(receipt) ||
    !isSafeRelativePath(receipt.path) ||
    !isDigest(receipt.digest)
  ) {
    errors.push(location + " has an invalid retained receipt reference");
    return;
  }
  const path = resolve(ledgerDirectory, receipt.path);
  const relation = relative(ledgerDirectory, path);
  if (
    relation === "" ||
    relation === ".." ||
    relation.startsWith(".." + sep) ||
    relation.startsWith(".." + "/") ||
    relation.startsWith(".." + "\\")
  ) {
    errors.push(location + " receipt escapes the qualification directory");
    return;
  }
  try {
    if (sha256(readFileSync(path)) !== receipt.digest) {
      errors.push(location + " receipt digest does not match " + receipt.path);
    }
  } catch (error) {
    errors.push(
      location + " cannot read retained receipt " + receipt.path + ": " + error.message,
    );
  }
}

function checkPublication(publication, location) {
  if (!isObject(publication)) {
    errors.push(location + " must contain external publication evidence");
    return;
  }
  const allowed = new Set(["registry", "url", "receipt"]);
  for (const key of Object.keys(publication)) {
    if (!allowed.has(key)) {
      errors.push(location + " has an unknown property " + key);
    }
  }
  if (!isNonEmptyString(publication.registry)) {
    errors.push(location + " must name an external registry or publication channel");
  }
  if (!isNonEmptyString(publication.url) || !/^https:\/\/[^\s]+$/.test(publication.url)) {
    errors.push(location + " must name an externally verifiable https URL");
  }
  checkRetainedReceipt(publication.receipt, location + ".receipt");
}

function checkEvidence(evidence, location, cohortIndex) {
  if (!Array.isArray(evidence)) {
    errors.push(location + " must be an evidence array");
    return;
  }
  for (const [index, item] of evidence.entries()) {
    const evidenceLocation = location + "[" + index + "]";
    if (!isObject(item) || !isObject(item.source_ref)) {
      errors.push(evidenceLocation + " must contain a source_ref");
      continue;
    }
    checkSourceRefs([item.source_ref], evidenceLocation + ".source_ref", "evidence", cohortIndex);
    if (item.receipt !== undefined) {
      checkRetainedReceipt(item.receipt, evidenceLocation);
    }
  }
}

let ledger;
try {
  ledger = parseJsonWithUniqueObjectKeys(readFileSync(ledgerPath, "utf8"));
} catch (error) {
  console.error("Cannot parse qualification ledger: " + error.message);
  process.exit(1);
}

if (!isObject(ledger) || !isObject(ledger.records)) {
  console.error("Ledger must contain an ID-keyed records object.");
  process.exit(1);
}

const cohortIndex = loadCohortIndex(cohortDirectory);
const seenTuples = new Map();
for (const [recordId, record] of Object.entries(ledger.records)) {
  const location = "records." + recordId;
  if (!/^[a-z0-9][a-z0-9.-]*$/.test(recordId)) {
    errors.push(location + " has an invalid record ID");
    continue;
  }
  if (!isObject(record) || !isObject(record.subject)) {
    errors.push(location + " must contain a subject object");
    continue;
  }
  if (!isNonEmptyString(record.subject.id)) {
    errors.push(location + ".subject.id must be a non-empty string");
    continue;
  }
  if (!Array.isArray(record.qualifications)) {
    errors.push(location + ".qualifications must be an array");
    continue;
  }

  if (!isObject(record.design)) {
    errors.push(location + ".design must be an object");
  } else if (record.design.status === "designed") {
    checkSourceRefs(
      record.design.decision_refs,
      location + ".design.decision_refs",
      "design",
      cohortIndex,
    );
  } else if (Array.isArray(record.design.decision_refs) && record.design.decision_refs.length > 0) {
    checkSourceRefs(
      record.design.decision_refs,
      location + ".design.decision_refs",
      "design",
      cohortIndex,
    );
  }

  if (!isObject(record.implementation)) {
    errors.push(location + ".implementation must be an object");
  } else if (record.implementation.status === "implemented") {
    checkSourceRefs(
      record.implementation.source_refs,
      location + ".implementation.source_refs",
      "implementation",
      cohortIndex,
    );
  } else if (
    Array.isArray(record.implementation.source_refs) &&
    record.implementation.source_refs.length > 0
  ) {
    checkSourceRefs(
      record.implementation.source_refs,
      location + ".implementation.source_refs",
      "implementation",
      cohortIndex,
    );
  }

  if (!Array.isArray(record.releases)) {
    errors.push(location + ".releases must be an array");
  } else {
    for (const [index, release] of record.releases.entries()) {
      const releaseLocation = location + ".releases[" + index + "]";
      if (!isObject(release)) {
        errors.push(releaseLocation + " must be an object");
        continue;
      }
      if (release.status !== "released") {
        errors.push(releaseLocation + " must use status released");
      }
      checkSourceRefs(release.source_refs, releaseLocation + ".source_refs", "release", cohortIndex);
      checkEvidence(release.evidence, releaseLocation + ".evidence", cohortIndex);
      checkPublication(release.publication, releaseLocation + ".publication");
    }
  }

  for (const [index, qualification] of record.qualifications.entries()) {
    const qualificationLocation = location + ".qualifications[" + index + "]";
    if (!isObject(qualification) || !isObject(qualification.combination)) {
      errors.push(qualificationLocation + " must contain a combination object");
      continue;
    }

    const environment = qualification.combination.environment;
    if (
      !isObject(environment) ||
      !isNonEmptyString(environment.id) ||
      !isNonEmptyString(environment.classification)
    ) {
      errors.push(qualificationLocation + " has an invalid environment");
      continue;
    }

    const infrastructure = normalizedInfrastructure(
      qualification.combination.infrastructure,
      qualificationLocation + ".combination.infrastructure",
    );
    const sourceRefs = checkSourceRefs(
      qualification.source_refs,
      qualificationLocation + ".source_refs",
      qualification.level === "local" ? "local" : "target-or-production",
      cohortIndex,
    );
    if (infrastructure === null || sourceRefs === null) {
      continue;
    }
    checkEvidence(qualification.evidence, qualificationLocation + ".evidence", cohortIndex);

    const tuple = JSON.stringify(
      canonicalize({
        subject: record.subject.id,
        source_refs: sourceRefs,
        level: qualification.level,
        environment,
        infrastructure,
      }),
    );
    const previous = seenTuples.get(tuple);
    if (previous !== undefined) {
      errors.push(
        qualificationLocation +
          " duplicates the Environment and Infrastructure qualification at " +
          previous,
      );
      continue;
    }
    seenTuples.set(tuple, qualificationLocation);
  }
}

if (errors.length > 0) {
  console.error(errors.join("\n"));
  process.exit(1);
}

console.log(
  "Qualification ledger semantic uniqueness check passed for " +
    Object.keys(ledger.records).length +
    " records.",
);
