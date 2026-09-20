import { createHash } from "node:crypto";
import { lstatSync, readFileSync, readlinkSync } from "node:fs";
import { resolve, relative, sep } from "node:path";
import { spawnSync } from "node:child_process";

export const COHORT_STAGES = Object.freeze([
  "source-closure",
  "artifact-digest",
  "clean-room-install",
  "real-package-install",
  "startup",
  "shutdown",
  "upgrade",
]);

export const SNAPSHOT_ALGORITHM = "lenso.git-worktree-snapshot-v1";

const SHA256_DIGEST = /^sha256:[0-9a-f]{64}$/;
const GIT_SHA = /^[0-9a-f]{40}$/;
const COHORT_ID = /^[a-z0-9][a-z0-9.-]*$/;
const SAFE_RELATIVE_PATH = /^(?!\/)(?!.*(?:^|\/)\.\.(?:\/|$)).+$/;

export function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export function isNonEmptyString(value) {
  return typeof value === "string" && value.length > 0;
}

export function sha256(value) {
  return "sha256:" + createHash("sha256").update(value).digest("hex");
}

export function isDigest(value) {
  return typeof value === "string" && SHA256_DIGEST.test(value);
}

export function isGitSha(value) {
  return typeof value === "string" && GIT_SHA.test(value);
}

export function isCohortId(value) {
  return typeof value === "string" && COHORT_ID.test(value);
}

export function isSafeRelativePath(value) {
  return typeof value === "string" && SAFE_RELATIVE_PATH.test(value);
}

/**
 * Sort source paths and JSON keys by JavaScript code units, not the host
 * locale. A cohort digest must not change merely because its verifier has a
 * different collation setting.
 */
export function compareCodeUnits(left, right) {
  if (left < right) return -1;
  if (left > right) return 1;
  return 0;
}

/**
 * JSON.parse silently accepts duplicate keys. A status or receipt is an
 * authority-bearing document, so reject them before a later key can erase an
 * earlier fact.
 */
export function parseJsonWithUniqueObjectKeys(source) {
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

export function canonicalize(value) {
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

function git(root, arguments_) {
  const result = spawnSync("git", ["-C", root, ...arguments_], {
    encoding: null,
  });
  if (result.status !== 0) {
    const detail = Buffer.from(result.stderr ?? "").toString("utf8").trim();
    throw new Error(
      "git " + arguments_.join(" ") + " failed for " + root +
        (detail.length === 0 ? "" : ": " + detail),
    );
  }
  return Buffer.from(result.stdout ?? "");
}

function safePathInside(root, candidate) {
  const resolvedRoot = resolve(root);
  const resolvedCandidate = resolve(resolvedRoot, candidate);
  const relation = relative(resolvedRoot, resolvedCandidate);
  if (
    relation === "" ||
    relation === ".." ||
    relation.startsWith(".." + sep) ||
    relation.startsWith(".." + "/") ||
    relation.startsWith(".." + "\\")
  ) {
    throw new Error("untracked path escapes repository root: " + candidate);
  }
  return resolvedCandidate;
}

function trackedDelta(root, revision) {
  return git(root, ["diff", "--no-ext-diff", "--binary", revision, "--"]);
}

function untrackedFiles(root) {
  const listed = git(root, ["ls-files", "--others", "--exclude-standard", "-z"])
    .toString("utf8")
    .split("\0")
    .filter((path) => path.length > 0)
    .sort(compareCodeUnits);

  return listed.map((path) => {
    const resolved = safePathInside(root, path);
    const stat = lstatSync(resolved);
    let bytes;
    let mode;
    if (stat.isFile()) {
      bytes = readFileSync(resolved);
      mode = stat.mode & 0o777;
    } else if (stat.isSymbolicLink()) {
      bytes = Buffer.from(readlinkSync(resolved), "utf8");
      mode = "symlink";
    } else {
      throw new Error("untracked path is not a file or symbolic link: " + path);
    }
    return {
      path,
      mode,
      digest: sha256(bytes),
    };
  });
}

/**
 * The snapshot is intentionally relative to an immutable Git revision. The
 * digest includes the binary tracked delta plus every non-ignored untracked
 * file, so it identifies a local candidate without pretending that it has an
 * immutable committed source revision.
 */
export function workingTreeSnapshot(root) {
  const normalizedRoot = resolve(root);
  const revision = git(normalizedRoot, ["rev-parse", "HEAD"])
    .toString("utf8")
    .trim();
  if (!isGitSha(revision)) {
    throw new Error("repository HEAD is not a full Git SHA: " + normalizedRoot);
  }
  const trackedPatch = trackedDelta(normalizedRoot, revision);
  const untracked = untrackedFiles(normalizedRoot);
  const payload = {
    algorithm: SNAPSHOT_ALGORITHM,
    revision,
    tracked_patch_digest: sha256(trackedPatch),
    untracked,
  };
  const kind =
    trackedPatch.length === 0 && untracked.length === 0 ? "committed" : "working-tree";
  return {
    revision,
    snapshot: {
      kind,
      algorithm: SNAPSHOT_ALGORITHM,
      digest: sha256(JSON.stringify(payload)),
    },
    detail: payload,
  };
}

function validLimitations(value) {
  return Array.isArray(value) && value.every(isNonEmptyString);
}

function validReceipt(receipt, manifestDirectory, errors, location, verifyReceipts) {
  if (
    !isObject(receipt) ||
    !isSafeRelativePath(receipt.path) ||
    !isDigest(receipt.digest) ||
    !isNonEmptyString(receipt.assertion)
  ) {
    errors.push(location + " must name a safe receipt path, digest, and assertion");
    return;
  }
  if (!verifyReceipts || manifestDirectory === undefined) {
    return;
  }
  const path = resolve(manifestDirectory, receipt.path);
  const relationship = relative(manifestDirectory, path);
  if (relationship === "" || relationship === ".." || relationship.startsWith(".." + sep)) {
    errors.push(location + " escapes its cohort directory");
    return;
  }
  try {
    if (sha256(readFileSync(path)) !== receipt.digest) {
      errors.push(location + " digest does not match " + receipt.path);
    }
  } catch (error) {
    errors.push(location + " cannot read " + receipt.path + ": " + error.message);
  }
}

function validatePublications(
  publications,
  coordinates,
  manifestDirectory,
  errors,
  verifyReceipts,
) {
  if (!Array.isArray(publications) || publications.length === 0) {
    errors.push("Published cohort must name a publication receipt for every artifact");
    return;
  }
  const publishedCoordinates = new Set();
  for (const [index, publication] of publications.entries()) {
    const location = "publications[" + index + "]";
    const allowed = new Set(["coordinate", "registry", "url", "receipt"]);
    if (!isObject(publication)) {
      errors.push(location + " must be an object");
      continue;
    }
    for (const key of Object.keys(publication)) {
      if (!allowed.has(key)) {
        errors.push(location + " has an unknown property " + key);
      }
    }
    if (!isNonEmptyString(publication.coordinate)) {
      errors.push(location + " must name an artifact coordinate");
    } else if (!coordinates.has(publication.coordinate)) {
      errors.push(location + " names an artifact outside artifact_closure");
    } else if (publishedCoordinates.has(publication.coordinate)) {
      errors.push(location + " repeats publication coordinate " + publication.coordinate);
    } else {
      publishedCoordinates.add(publication.coordinate);
    }
    if (!isNonEmptyString(publication.registry)) {
      errors.push(location + " must name an external registry or publication channel");
    }
    if (
      !isNonEmptyString(publication.url) ||
      !/^https:\/\/[^\s]+$/.test(publication.url)
    ) {
      errors.push(location + " must name an externally verifiable https URL");
    }
    validReceipt(
      publication.receipt,
      manifestDirectory,
      errors,
      location + ".receipt",
      verifyReceipts,
    );
  }
  for (const coordinate of coordinates) {
    if (!publishedCoordinates.has(coordinate)) {
      errors.push("Published cohort has no publication receipt for " + coordinate);
    }
  }
}

function validateStage(stageName, stage, cohort, manifestDirectory, errors, verifyReceipts) {
  const location = "stages." + stageName;
  if (!isObject(stage)) {
    errors.push(location + " must be an object");
    return;
  }
  const allowed = new Set([
    "status",
    "receipts",
    "known_limitations",
    "not_applicable_reason",
  ]);
  for (const key of Object.keys(stage)) {
    if (!allowed.has(key)) {
      errors.push(location + " has an unknown property " + key);
    }
  }
  if (!["passed", "not-run", "blocked", "not-applicable"].includes(stage.status)) {
    errors.push(location + " has an invalid status");
    return;
  }
  if (stage.status === "not-applicable") {
    if (!isNonEmptyString(stage.not_applicable_reason)) {
      errors.push(location + " must explain why the stage is not applicable");
    }
  } else if (stage.not_applicable_reason !== undefined) {
    errors.push(location + " may name not_applicable_reason only when status is not-applicable");
  }
  if (stage.status === "passed") {
    if (!Array.isArray(stage.receipts) || stage.receipts.length === 0) {
      errors.push(location + " passed without a receipt");
    } else {
      stage.receipts.forEach((receipt, index) =>
        validReceipt(
          receipt,
          manifestDirectory,
          errors,
          location + ".receipts[" + index + "]",
          verifyReceipts,
        ),
      );
    }
  } else if (!validLimitations(stage.known_limitations) || stage.known_limitations.length === 0) {
    errors.push(location + " must explain why it is not passed");
  }
  if (stage.status === "passed" && stageName === "artifact-digest" && cohort.artifact_closure.length === 0) {
    errors.push(location + " passed without an artifact closure");
  }
  if (
    stage.status === "passed" &&
    ["clean-room-install", "real-package-install", "startup", "shutdown", "upgrade"].includes(stageName) &&
    cohort.stages?.["artifact-digest"]?.status !== "passed"
  ) {
    errors.push(location + " passed before artifact-digest passed");
  }
}

function sourceKey(source) {
  return [source.repository, source.revision, source.snapshot.digest].join("\u0000");
}

/**
 * Return structural errors rather than throwing so callers can compose cohort
 * checks with the qualification ledger check.
 */
export function validateReleaseCohort(cohort, options = {}) {
  const errors = [];
  const manifestDirectory = options.manifestDirectory;
  const verifyReceipts = options.verifyReceipts !== false;
  if (!isObject(cohort)) {
    return ["Cohort must be a JSON object"];
  }
  const allowedTopLevel = new Set([
    "$schema",
    "schema_version",
    "id",
    "state",
    "source_closure",
    "artifact_closure",
    "stages",
    "known_limitations",
    "publications",
  ]);
  for (const key of Object.keys(cohort)) {
    if (!allowedTopLevel.has(key)) {
      errors.push("Cohort has an unknown property " + key);
    }
  }
  if (cohort.$schema !== "../release-cohort.schema.json") {
    errors.push("Cohort must use ../release-cohort.schema.json");
  }
  if (cohort.schema_version !== 1) {
    errors.push("Cohort schema_version must be 1");
  }
  if (!isCohortId(cohort.id)) {
    errors.push("Cohort ID is invalid");
  }
  if (!["candidate", "release-ready", "published"].includes(cohort.state)) {
    errors.push("Cohort state must be candidate, release-ready, or published");
  }
  if (!Array.isArray(cohort.source_closure) || cohort.source_closure.length === 0) {
    errors.push("Cohort must contain a non-empty source_closure");
  }
  if (!Array.isArray(cohort.artifact_closure)) {
    errors.push("Cohort artifact_closure must be an array");
  }
  if (!isObject(cohort.stages)) {
    errors.push("Cohort stages must be an object");
  }
  if (!validLimitations(cohort.known_limitations)) {
    errors.push("Cohort known_limitations must be an array of strings");
  }
  if (errors.length > 0) {
    return errors;
  }

  const repositories = new Set();
  const sources = new Map();
  for (const [index, source] of cohort.source_closure.entries()) {
    const location = "source_closure[" + index + "]";
    const allowed = new Set(["repository", "revision", "snapshot", "coordinates"]);
    if (!isObject(source)) {
      errors.push(location + " must be an object");
      continue;
    }
    for (const key of Object.keys(source)) {
      if (!allowed.has(key)) {
        errors.push(location + " has an unknown property " + key);
      }
    }
    if (!isNonEmptyString(source.repository) || !isGitSha(source.revision)) {
      errors.push(location + " must name repository and full source revision");
      continue;
    }
    if (repositories.has(source.repository)) {
      errors.push(location + " repeats repository " + source.repository);
    }
    repositories.add(source.repository);
    if (
      !isObject(source.snapshot) ||
      !["committed", "working-tree"].includes(source.snapshot.kind) ||
      source.snapshot.algorithm !== SNAPSHOT_ALGORITHM ||
      !isDigest(source.snapshot.digest)
    ) {
      errors.push(location + " has an invalid source snapshot");
      continue;
    }
    if (
      !Array.isArray(source.coordinates) ||
      source.coordinates.length === 0 ||
      !source.coordinates.every(isNonEmptyString) ||
      new Set(source.coordinates).size !== source.coordinates.length
    ) {
      errors.push(location + " must name unique package or artifact coordinates");
    }
    if (
      ["release-ready", "published"].includes(cohort.state) &&
      source.snapshot.kind !== "committed"
    ) {
      errors.push(location + " makes a " + cohort.state + " cohort depend on a working tree");
    }
    sources.set(sourceKey(source), source);
  }

  const coordinates = new Set();
  for (const [index, artifact] of cohort.artifact_closure.entries()) {
    const location = "artifact_closure[" + index + "]";
    const allowed = new Set([
      "coordinate",
      "kind",
      "source",
      "digest",
      "bytes",
    ]);
    if (!isObject(artifact)) {
      errors.push(location + " must be an object");
      continue;
    }
    for (const key of Object.keys(artifact)) {
      if (!allowed.has(key)) {
        errors.push(location + " has an unknown property " + key);
      }
    }
    if (!isNonEmptyString(artifact.coordinate)) {
      errors.push(location + " must have a coordinate");
    } else if (coordinates.has(artifact.coordinate)) {
      errors.push(location + " repeats artifact coordinate " + artifact.coordinate);
    } else {
      coordinates.add(artifact.coordinate);
    }
    if (!["cargo-crate", "npm-tarball", "binary", "wasm", "bundle"].includes(artifact.kind)) {
      errors.push(location + " has an invalid artifact kind");
    }
    if (
      !isObject(artifact.source) ||
      !isNonEmptyString(artifact.source.repository) ||
      !isGitSha(artifact.source.revision) ||
      !isDigest(artifact.source.snapshot_digest)
    ) {
      errors.push(location + " has an invalid artifact source");
    } else {
      const source = sources.get(
        [
          artifact.source.repository,
          artifact.source.revision,
          artifact.source.snapshot_digest,
        ].join("\u0000"),
      );
      if (source === undefined) {
        errors.push(location + " does not point at a source_closure entry");
      } else if (!source.coordinates.includes(artifact.coordinate)) {
        errors.push(location + " is not declared by its source_closure entry");
      }
    }
    if (!isDigest(artifact.digest) || !Number.isSafeInteger(artifact.bytes) || artifact.bytes < 0) {
      errors.push(location + " must have a sha256 digest and non-negative size");
    }
  }

  const stageNames = Object.keys(cohort.stages);
  for (const stage of COHORT_STAGES) {
    if (!(stage in cohort.stages)) {
      errors.push("Cohort is missing required stage " + stage);
    } else {
      validateStage(stage, cohort.stages[stage], cohort, manifestDirectory, errors, verifyReceipts);
    }
  }
  for (const stage of stageNames) {
    if (!COHORT_STAGES.includes(stage)) {
      errors.push("Cohort has an unknown stage " + stage);
    }
  }
  if (cohort.stages?.["source-closure"]?.status !== "passed") {
    errors.push("Cohort source-closure must pass before it can be referenced");
  }
  if (["release-ready", "published"].includes(cohort.state)) {
    for (const stage of COHORT_STAGES) {
      const status = cohort.stages?.[stage]?.status;
      if (stage === "upgrade" && status === "not-applicable") {
        continue;
      }
      if (status !== "passed") {
        errors.push(cohort.state + " cohort has not passed " + stage);
      }
    }
    if (cohort.artifact_closure.length === 0) {
      errors.push(cohort.state + " cohort has no artifacts");
    }
  }
  if (cohort.state === "published") {
    validatePublications(
      cohort.publications,
      coordinates,
      manifestDirectory,
      errors,
      verifyReceipts,
    );
  } else if (cohort.publications !== undefined) {
    errors.push(cohort.state + " cohort must not claim external publication");
  }
  return errors;
}

export function releaseCohortSources(cohort) {
  if (!isObject(cohort) || !Array.isArray(cohort.source_closure)) {
    return [];
  }
  return cohort.source_closure.map((source) => ({
    repository: source.repository,
    revision: source.revision,
    snapshot_kind: source.snapshot?.kind,
    snapshot_digest: source.snapshot?.digest,
  }));
}

export function verifyCohortWorkspaces(cohort, workspaceByRepository) {
  const errors = [];
  for (const source of cohort.source_closure) {
    const workspace = workspaceByRepository.get(source.repository);
    if (workspace === undefined) {
      errors.push("No --workspace mapping supplied for " + source.repository);
      continue;
    }
    let actual;
    try {
      actual = workingTreeSnapshot(workspace);
    } catch (error) {
      errors.push("Cannot snapshot " + source.repository + ": " + error.message);
      continue;
    }
    if (actual.revision !== source.revision) {
      errors.push(
        source.repository + " is at " + actual.revision + " instead of " + source.revision,
      );
    }
    if (actual.snapshot.kind !== source.snapshot.kind) {
      errors.push(
        source.repository + " snapshot kind is " + actual.snapshot.kind +
          " instead of " + source.snapshot.kind,
      );
    }
    if (actual.snapshot.digest !== source.snapshot.digest) {
      errors.push(source.repository + " source snapshot digest does not match cohort");
    }
  }
  return errors;
}

export function verifyCohortArtifacts(cohort, artifactByCoordinate) {
  const errors = [];
  for (const artifact of cohort.artifact_closure) {
    const path = artifactByCoordinate.get(artifact.coordinate);
    if (path === undefined) {
      errors.push("No --artifact mapping supplied for " + artifact.coordinate);
      continue;
    }
    try {
      const bytes = readFileSync(path);
      if (bytes.length !== artifact.bytes) {
        errors.push(artifact.coordinate + " size does not match cohort");
      }
      if (sha256(bytes) !== artifact.digest) {
        errors.push(artifact.coordinate + " digest does not match cohort");
      }
    } catch (error) {
      errors.push("Cannot read artifact " + artifact.coordinate + ": " + error.message);
    }
  }
  return errors;
}
