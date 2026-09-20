import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { sha256 } from "./release-cohort-lib.mjs";

const checkerPath = fileURLToPath(
  new URL("./check-qualification-ledger.mjs", import.meta.url),
);
const checkedInLedgerPath = fileURLToPath(
  new URL("../docs/qualification/qualification-status.json", import.meta.url),
);

const REQUIRED_TASK_ONE_FOCUS_RECORDS = Object.freeze([
  "lenso.web.workers-http",
  "lenso.web.workers-stream",
  "lenso.web.workers-websocket",
  "lenso.runtime.workers-runtime",
  "lenso.runtime.workers-w01",
  "lenso.runtime.workers-w02",
  "lenso.auth.d1-storage",
  "lenso.auth.postgresql-storage",
  "lenso.auth.workers-d1",
  "lenso.auth.workers-hyperdrive-postgresql",
  "lenso.runtime.browser",
  "lenso.runtime.wasip2",
  "lenso.runtime.bun",
  "lenso.runtime.process",
  "lenso.runtime.remote-adapter",
]);

function expectRejected(source, message) {
  const result = spawnSync(process.execPath, [checkerPath, "/dev/stdin"], {
    input: source,
    encoding: "utf8",
  });

  assert.notEqual(result.status, 0, result.stdout);
  assert.match(result.stderr, message);
}

function write(path, content) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
}

test("keeps every Task 1 target-sensitive focus in the canonical inventory", () => {
  const ledger = JSON.parse(readFileSync(checkedInLedgerPath, "utf8"));

  for (const recordId of REQUIRED_TASK_ONE_FOCUS_RECORDS) {
    assert.ok(ledger.records[recordId], "missing Task 1 focus record " + recordId);
  }

  for (const recordId of REQUIRED_TASK_ONE_FOCUS_RECORDS.filter(
    (id) => id !== "lenso.runtime.workers-w02",
  )) {
    const record = ledger.records[recordId];
    assert.equal(record.design.status, "not_assessed", recordId + " design status");
    assert.deepEqual(record.design.decision_refs, [], recordId + " design refs");
    assert.equal(
      record.implementation.status,
      "not_assessed",
      recordId + " implementation status",
    );
    assert.deepEqual(record.implementation.source_refs, [], recordId + " source refs");
    assert.deepEqual(record.releases, [], recordId + " releases");
    assert.deepEqual(record.qualifications, [], recordId + " qualifications");
  }

  assert.equal(ledger.records["lenso.runtime.workers-w02"].implementation.status, "implemented");
  assert.deepEqual(
    ledger.records["lenso.runtime.workers-w02"].qualifications.map((entry) => entry.level),
    ["local"],
  );
  assert.deepEqual(
    ledger.records["lenso.auth.workers-g4"].qualifications.map((entry) => entry.level),
    ["target"],
  );
});

test("rejects a repeated records object key before JSON parsing overwrites it", () => {
  expectRejected(
    "{\"records\":{\"duplicate\":{\"subject\":{\"id\":\"subject\"},\"qualifications\":[]},\"duplicate\":{\"subject\":{\"id\":\"subject\"},\"qualifications\":[]}}}",
    /duplicate object key "duplicate"/,
  );
});

test("rejects repeated top-level records properties", () => {
  expectRejected(
    "{\"records\":{\"first\":{\"subject\":{\"id\":\"subject\"},\"qualifications\":[]}},\"records\":{\"duplicate\":{\"subject\":{\"id\":\"subject\"},\"qualifications\":[]},\"duplicate\":{\"subject\":{\"id\":\"subject\"},\"qualifications\":[]}}}",
    /duplicate object key "records"/,
  );
});

test("rejects a qualification tuple duplicated with reordered infrastructure", () => {
  expectRejected(
    JSON.stringify({
      records: {
        record: {
          subject: { id: "subject" },
          qualifications: [
            {
              level: "local",
              combination: {
                environment: {
                  id: "simulated",
                  classification: "test",
                },
                infrastructure: [
                  { role: "clock", implementation: "fixed" },
                  { role: "store", implementation: "memory" },
                ],
              },
              source_refs: [
                {
                  repository: "owner/repository",
                  revision: "a".repeat(40),
                  paths: ["src/example"],
                },
              ],
            },
            {
              level: "local",
              combination: {
                environment: {
                  id: "simulated",
                  classification: "test",
                },
                infrastructure: [
                  { role: "store", implementation: "memory" },
                  { role: "clock", implementation: "fixed" },
                ],
              },
              source_refs: [
                {
                  repository: "owner/repository",
                  revision: "a".repeat(40),
                  paths: ["src/example"],
                },
              ],
            },
          ],
        },
      },
    }),
    /duplicates the Environment and Infrastructure qualification/,
  );
});

test("rejects a repeated infrastructure role in one qualification", () => {
  expectRejected(
    JSON.stringify({
      records: {
        record: {
          subject: { id: "subject" },
          qualifications: [
            {
              level: "local",
              combination: {
                environment: {
                  id: "simulated",
                  classification: "test",
                },
                infrastructure: [
                  { role: "store", implementation: "memory" },
                  { role: "store", implementation: "other-memory" },
                ],
              },
              source_refs: [
                {
                  repository: "owner/repository",
                  revision: "a".repeat(40),
                  paths: ["src/example"],
                },
              ],
            },
          ],
        },
      },
    }),
    /repeats infrastructure role store/,
  );
});

test("rejects a repeated source reference in one qualification", () => {
  const sourceRef = {
    repository: "owner/repository",
    revision: "a".repeat(40),
    paths: ["src/example"],
  };
  expectRejected(
    JSON.stringify({
      records: {
        record: {
          subject: { id: "subject" },
          qualifications: [
            {
              level: "local",
              combination: {
                environment: {
                  id: "simulated",
                  classification: "test",
                },
                infrastructure: [
                  { role: "store", implementation: "memory" },
                ],
              },
              source_refs: [sourceRef, sourceRef],
            },
          ],
        },
      },
    }),
    /repeats a source reference/,
  );
});

test("links a local candidate only to the retained matching source snapshot", () => {
  const root = mkdtempSync(join(tmpdir(), "lenso-ledger-cohort-"));
  const cohortDirectory = join(root, "cohorts");
  const receipt = join(cohortDirectory, "receipts", "source.json");
  write(receipt, "{\"source\":\"closed\"}\n");
  const snapshotDigest = "sha256:" + "b".repeat(64);
  const source = {
    repository: "owner/repository",
    revision: "a".repeat(40),
    snapshot: {
      kind: "working-tree",
      algorithm: "lenso.git-worktree-snapshot-v1",
      digest: snapshotDigest,
    },
    coordinates: ["cargo:owner@1.0.0"],
  };
  const cohort = {
    $schema: "../release-cohort.schema.json",
    schema_version: 1,
    id: "candidate.fixture",
    state: "candidate",
    source_closure: [source],
    artifact_closure: [],
    stages: {
      "source-closure": {
        status: "passed",
        receipts: [
          {
            path: "receipts/source.json",
            digest: sha256(readFileSync(receipt)),
            assertion: "Source closure was captured.",
          },
        ],
      },
      "artifact-digest": { status: "not-run", known_limitations: ["No artifacts yet."] },
      "clean-room-install": { status: "not-run", known_limitations: ["No artifacts yet."] },
      "real-package-install": { status: "not-run", known_limitations: ["No artifacts yet."] },
      startup: { status: "not-run", known_limitations: ["No artifact host yet."] },
      shutdown: { status: "not-run", known_limitations: ["No artifact host yet."] },
      upgrade: { status: "not-run", known_limitations: ["No artifact migration yet."] },
    },
    known_limitations: ["Candidate is not release-ready or published."],
  };
  write(join(cohortDirectory, "candidate.json"), JSON.stringify(cohort, null, 2) + "\n");
  const sourceRef = {
    repository: source.repository,
    revision: source.revision,
    paths: ["src/example"],
    cohort: { id: cohort.id, snapshot_digest: snapshotDigest },
  };
  const ledger = {
    records: {
      "example.candidate": {
        subject: { id: "example.subject" },
        design: { status: "not_assessed", decision_refs: [] },
        implementation: { status: "implemented", source_refs: [sourceRef] },
        releases: [],
        qualifications: [
          {
            level: "local",
            combination: {
              environment: { id: "simulated", classification: "test" },
              infrastructure: [{ role: "store", implementation: "memory" }],
            },
            source_refs: [sourceRef],
            evidence: [
              {
                kind: "test",
                source_ref: sourceRef,
                assertion: "Local candidate receipt.",
                receipt: {
                  path: "cohorts/receipts/source.json",
                  digest: sha256(readFileSync(receipt)),
                },
              },
            ],
          },
        ],
      },
    },
  };
  const ledgerPath = join(root, "ledger.json");
  write(ledgerPath, JSON.stringify(ledger, null, 2) + "\n");
  const passing = spawnSync(
    process.execPath,
    [checkerPath, ledgerPath, "--cohort-directory", cohortDirectory],
    { encoding: "utf8" },
  );
  assert.equal(passing.status, 0, passing.stderr);

  ledger.records["example.candidate"].qualifications[0].level = "target";
  write(ledgerPath, JSON.stringify(ledger, null, 2) + "\n");
  const rejected = spawnSync(
    process.execPath,
    [checkerPath, ledgerPath, "--cohort-directory", cohortDirectory],
    { encoding: "utf8" },
  );
  assert.notEqual(rejected.status, 0, rejected.stdout);
  assert.match(rejected.stderr, /mutable or non-release-ready cohort for a target or production claim/);
});

test("requires an external publication receipt for a Released ledger facet", () => {
  const root = mkdtempSync(join(tmpdir(), "lenso-ledger-publication-"));
  const cohortDirectory = join(root, "cohorts");
  mkdirSync(cohortDirectory);
  const cohortReceipt = join(cohortDirectory, "receipts", "source.json");
  write(cohortReceipt, "{\"source\":\"closed\"}\n");
  const snapshotDigest = "sha256:" + "b".repeat(64);
  const source = {
    repository: "owner/repository",
    revision: "a".repeat(40),
    snapshot: {
      kind: "committed",
      algorithm: "lenso.git-worktree-snapshot-v1",
      digest: snapshotDigest,
    },
    coordinates: ["cargo:example@1.0.0"],
  };
  const completedStages = Object.fromEntries(
    [
      "source-closure",
      "artifact-digest",
      "clean-room-install",
      "real-package-install",
      "startup",
      "shutdown",
      "upgrade",
    ].map((name) => [
      name,
      {
        status: "passed",
        receipts: [
          {
            path: "receipts/source.json",
            digest: sha256(readFileSync(cohortReceipt)),
            assertion: name + " receipt",
          },
        ],
      },
    ]),
  );
  const releaseReadyCohort = {
    $schema: "../release-cohort.schema.json",
    schema_version: 1,
    id: "release-ready.fixture",
    state: "release-ready",
    source_closure: [source],
    artifact_closure: [
      {
        coordinate: "cargo:example@1.0.0",
        kind: "cargo-crate",
        source: {
          repository: source.repository,
          revision: source.revision,
          snapshot_digest: snapshotDigest,
        },
        digest: "sha256:" + "c".repeat(64),
        bytes: 1,
      },
    ],
    stages: completedStages,
    known_limitations: ["No external publication has been verified."],
  };
  const cohortPath = join(cohortDirectory, "release-ready.json");
  write(cohortPath, JSON.stringify(releaseReadyCohort, null, 2) + "\n");
  const sourceRef = {
    repository: source.repository,
    revision: source.revision,
    paths: ["src/example"],
    cohort: {
      id: releaseReadyCohort.id,
      snapshot_digest: snapshotDigest,
    },
  };
  const ledger = {
    records: {
      "example.published": {
        subject: {
          kind: "capability",
          id: "example.published@1",
          title: "Published example",
        },
        design: { status: "not_assessed", decision_refs: [] },
        implementation: { status: "implemented", source_refs: [sourceRef] },
        releases: [
          {
            status: "released",
            artifact: "cargo:example",
            version: "1.0.0",
            source_refs: [sourceRef],
            evidence: [
              {
                kind: "artifact",
                source_ref: sourceRef,
                assertion: "Artifact was prepared.",
              },
            ],
            known_limitations: ["No target qualification is implied."],
          },
        ],
        qualifications: [],
        known_limitations: ["No target qualification is implied."],
      },
    },
  };
  const ledgerPath = join(root, "ledger.json");
  write(ledgerPath, JSON.stringify(ledger, null, 2) + "\n");
  const missingPublication = spawnSync(
    process.execPath,
    [checkerPath, ledgerPath, "--cohort-directory", cohortDirectory],
    { encoding: "utf8" },
  );
  assert.notEqual(missingPublication.status, 0, missingPublication.stdout);
  assert.match(missingPublication.stderr, /must contain external publication evidence/);

  const publicationReceipt = join(root, "receipts", "publication.json");
  write(
    publicationReceipt,
    JSON.stringify({
      registry: "example registry",
      url: "https://registry.example.test/cargo/example/1.0.0",
    }) + "\n",
  );
  ledger.records["example.published"].releases[0].publication = {
    registry: "example registry",
    url: "https://registry.example.test/cargo/example/1.0.0",
    receipt: {
      path: "receipts/publication.json",
      digest: sha256(readFileSync(publicationReceipt)),
    },
  };
  write(ledgerPath, JSON.stringify(ledger, null, 2) + "\n");
  const releaseReadyRejected = spawnSync(
    process.execPath,
    [checkerPath, ledgerPath, "--cohort-directory", cohortDirectory],
    { encoding: "utf8" },
  );
  assert.notEqual(releaseReadyRejected.status, 0, releaseReadyRejected.stdout);
  assert.match(releaseReadyRejected.stderr, /uses a non-published cohort for a Released claim/);

  const cohortPublicationReceipt = join(
    cohortDirectory,
    "receipts",
    "publication.json",
  );
  write(
    cohortPublicationReceipt,
    JSON.stringify({
      coordinate: "cargo:example@1.0.0",
      registry: "example registry",
      url: "https://registry.example.test/cargo/example/1.0.0",
    }) + "\n",
  );
  releaseReadyCohort.state = "published";
  releaseReadyCohort.publications = [
    {
      coordinate: "cargo:example@1.0.0",
      registry: "example registry",
      url: "https://registry.example.test/cargo/example/1.0.0",
      receipt: {
        path: "receipts/publication.json",
        digest: sha256(readFileSync(cohortPublicationReceipt)),
        assertion: "The exact artifact is externally available at the declared registry URL.",
      },
    },
  ];
  write(cohortPath, JSON.stringify(releaseReadyCohort, null, 2) + "\n");
  const published = spawnSync(
    process.execPath,
    [checkerPath, ledgerPath, "--cohort-directory", cohortDirectory],
    { encoding: "utf8" },
  );
  assert.equal(published.status, 0, published.stderr);
});
