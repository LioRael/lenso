import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  SNAPSHOT_ALGORITHM,
  compareCodeUnits,
  sha256,
  workingTreeSnapshot,
} from "./release-cohort-lib.mjs";

const checkerPath = fileURLToPath(
  new URL("./check-release-cohort.mjs", import.meta.url),
);

function command(directory, command, arguments_) {
  const result = spawnSync(command, arguments_, { cwd: directory, encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
}

function write(path, content) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
}

function stage(receiptPath, assertion) {
  const bytes = readFileSync(receiptPath);
  return {
    status: "passed",
    receipts: [
      {
        path: "receipts/" + receiptPath.split("/").at(-1),
        digest: sha256(bytes),
        assertion,
      },
    ],
  };
}

function candidateFixture({ dirty = true } = {}) {
  const root = mkdtempSync(join(tmpdir(), "lenso-release-cohort-"));
  const workspace = join(root, "source");
  mkdirSync(workspace);
  command(workspace, "git", ["init", "-q"]);
  command(workspace, "git", ["config", "user.email", "cohort@example.test"]);
  command(workspace, "git", ["config", "user.name", "Cohort Test"]);
  write(join(workspace, "source.txt"), "before\n");
  command(workspace, "git", ["add", "source.txt"]);
  command(workspace, "git", ["commit", "-qm", "base"]);
  if (dirty) {
    write(join(workspace, "source.txt"), "after\n");
    write(join(workspace, "new.txt"), "untracked\n");
  }
  const source = workingTreeSnapshot(workspace);
  assert.equal(source.snapshot.kind, dirty ? "working-tree" : "committed");
  assert.equal(source.snapshot.algorithm, SNAPSHOT_ALGORITHM);

  const cohortDirectory = join(root, "cohort");
  const receiptsDirectory = join(cohortDirectory, "receipts");
  const stages = {};
  for (const name of [
    "source-closure",
    "artifact-digest",
    "clean-room-install",
    "real-package-install",
    "startup",
    "shutdown",
    "upgrade",
  ]) {
    const receipt = join(receiptsDirectory, name + ".json");
    write(receipt, JSON.stringify({ stage: name, passed: true }) + "\n");
    stages[name] = stage(receipt, name + " receipt");
  }
  const artifactPath = join(root, "candidate.crate");
  write(artifactPath, "candidate artifact\n");
  const artifact = readFileSync(artifactPath);
  const cohort = {
    $schema: "../release-cohort.schema.json",
    schema_version: 1,
    id: "candidate.fixture",
    state: "candidate",
    source_closure: [
      {
        repository: "example/repository",
        revision: source.revision,
        snapshot: source.snapshot,
        coordinates: ["cargo:example@1.0.0"],
      },
    ],
    artifact_closure: [
      {
        coordinate: "cargo:example@1.0.0",
        kind: "cargo-crate",
        source: {
          repository: "example/repository",
          revision: source.revision,
          snapshot_digest: source.snapshot.digest,
        },
        digest: sha256(artifact),
        bytes: artifact.length,
      },
    ],
    stages,
    known_limitations: [
      dirty
        ? "Candidate uses an uncommitted source snapshot and is not release-ready or published."
        : "Candidate has a committed source snapshot but does not claim publication.",
    ],
  };
  const manifest = join(cohortDirectory, "cohort.json");
  write(manifest, JSON.stringify(cohort, null, 2) + "\n");
  return { artifactPath, cohort, manifest, workspace };
}

test("checks the source closure, artifact digest, and retained receipts together", () => {
  const fixture = candidateFixture();
  const result = spawnSync(
    process.execPath,
    [
      checkerPath,
      fixture.manifest,
      "--workspace",
      "example/repository=" + fixture.workspace,
      "--artifact",
      "cargo:example@1.0.0=" + fixture.artifactPath,
    ],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Release cohort check passed/);
});

test("fails closed when a source snapshot changes after its receipt", () => {
  const fixture = candidateFixture();
  write(join(fixture.workspace, "source.txt"), "later\n");
  const result = spawnSync(
    process.execPath,
    [checkerPath, fixture.manifest, "--workspace", "example/repository=" + fixture.workspace],
    { encoding: "utf8" },
  );
  assert.notEqual(result.status, 0, result.stdout);
  assert.match(result.stderr, /source snapshot digest does not match cohort/);
});

test("fails closed when an artifact is mutated after its digest receipt", () => {
  const fixture = candidateFixture();
  write(fixture.artifactPath, "mutated artifact\n");
  const result = spawnSync(
    process.execPath,
    [
      checkerPath,
      fixture.manifest,
      "--artifact",
      "cargo:example@1.0.0=" + fixture.artifactPath,
    ],
    { encoding: "utf8" },
  );
  assert.notEqual(result.status, 0, result.stdout);
  assert.match(result.stderr, /cargo:example@1\.0\.0 digest does not match cohort/);
});

test("never upgrades a working-tree cohort to release-ready", () => {
  const fixture = candidateFixture();
  fixture.cohort.state = "release-ready";
  write(fixture.manifest, JSON.stringify(fixture.cohort, null, 2) + "\n");
  const result = spawnSync(process.execPath, [checkerPath, fixture.manifest], {
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0, result.stdout);
  assert.match(result.stderr, /release-ready cohort depend on a working tree/);
});

test("rejects the retired released state", () => {
  const fixture = candidateFixture({ dirty: false });
  fixture.cohort.state = "released";
  write(fixture.manifest, JSON.stringify(fixture.cohort, null, 2) + "\n");
  const result = spawnSync(process.execPath, [checkerPath, fixture.manifest], {
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0, result.stdout);
  assert.match(result.stderr, /must be candidate, release-ready, or published/);
});

test("keeps a complete committed local closure release-ready until it is published", () => {
  const fixture = candidateFixture({ dirty: false });
  fixture.cohort.state = "release-ready";
  fixture.cohort.known_limitations = [
    "No external registry or publication channel has been verified.",
  ];
  write(fixture.manifest, JSON.stringify(fixture.cohort, null, 2) + "\n");
  const result = spawnSync(process.execPath, [checkerPath, fixture.manifest], {
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /release-ready/);
});

test("allows only an explicit stateless upgrade exemption in a release-ready cohort", () => {
  const fixture = candidateFixture({ dirty: false });
  fixture.cohort.state = "release-ready";
  fixture.cohort.stages.upgrade = {
    status: "not-applicable",
    known_limitations: ["This package has no persisted state or schema to upgrade."],
  };
  write(fixture.manifest, JSON.stringify(fixture.cohort, null, 2) + "\n");
  const missingReason = spawnSync(process.execPath, [checkerPath, fixture.manifest], {
    encoding: "utf8",
  });
  assert.notEqual(missingReason.status, 0, missingReason.stdout);
  assert.match(missingReason.stderr, /must explain why the stage is not applicable/);

  fixture.cohort.stages.upgrade.not_applicable_reason =
    "No persistent storage, schema, or migration route exists for this artifact.";
  write(fixture.manifest, JSON.stringify(fixture.cohort, null, 2) + "\n");
  const stateless = spawnSync(process.execPath, [checkerPath, fixture.manifest], {
    encoding: "utf8",
  });
  assert.equal(stateless.status, 0, stateless.stderr);

  fixture.cohort.stages.shutdown = {
    status: "not-applicable",
    known_limitations: ["Fixture intentionally skips a real shutdown."],
    not_applicable_reason: "Fixture only validates a pure library call.",
  };
  write(fixture.manifest, JSON.stringify(fixture.cohort, null, 2) + "\n");
  const lifecycleExemption = spawnSync(process.execPath, [checkerPath, fixture.manifest], {
    encoding: "utf8",
  });
  assert.notEqual(lifecycleExemption.status, 0, lifecycleExemption.stdout);
  assert.match(lifecycleExemption.stderr, /release-ready cohort has not passed shutdown/);
});

test("requires an external publication receipt before a cohort is published", () => {
  const fixture = candidateFixture({ dirty: false });
  fixture.cohort.state = "published";
  write(fixture.manifest, JSON.stringify(fixture.cohort, null, 2) + "\n");
  const missingPublication = spawnSync(process.execPath, [checkerPath, fixture.manifest], {
    encoding: "utf8",
  });
  assert.notEqual(missingPublication.status, 0, missingPublication.stdout);
  assert.match(missingPublication.stderr, /publication receipt for every artifact/);

  const publicationReceipt = join(
    dirname(fixture.manifest),
    "receipts",
    "publication.json",
  );
  write(
    publicationReceipt,
    JSON.stringify({
      coordinate: "cargo:example@1.0.0",
      registry: "example registry",
      url: "https://registry.example.test/cargo/example/1.0.0",
    }) + "\n",
  );
  fixture.cohort.publications = [
    {
      coordinate: "cargo:example@1.0.0",
      registry: "example registry",
      url: "https://registry.example.test/cargo/example/1.0.0",
      receipt: {
        path: "receipts/publication.json",
        digest: sha256(readFileSync(publicationReceipt)),
        assertion: "The exact artifact is externally available at the declared registry URL.",
      },
    },
  ];
  write(fixture.manifest, JSON.stringify(fixture.cohort, null, 2) + "\n");
  const published = spawnSync(process.execPath, [checkerPath, fixture.manifest], {
    encoding: "utf8",
  });
  assert.equal(published.status, 0, published.stderr);
  assert.match(published.stdout, /published/);
});

test("rejects a duplicate authority-bearing JSON key", () => {
  const root = mkdtempSync(join(tmpdir(), "lenso-release-cohort-invalid-"));
  const manifest = join(root, "cohort.json");
  write(manifest, "{\"id\":\"first\",\"id\":\"second\"}");
  const result = spawnSync(process.execPath, [checkerPath, manifest], {
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0, result.stdout);
  assert.match(result.stderr, /duplicate object key "id"/);
});

test("uses code-unit ordering rather than verifier locale for cohort digests", () => {
  assert.equal(compareCodeUnits("A", "a"), -1);
  assert.equal(compareCodeUnits("a", "A"), 1);
  assert.equal(compareCodeUnits("same", "same"), 0);
});
