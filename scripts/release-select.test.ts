import { expect, test } from "bun:test";
import { changedPublicVersion, isVersionMerge } from "./release-select";

const repository = "LioRael/lenso";
const sha = "a".repeat(40);
const pr = {
  merged_at: "2026-10-10T00:00:00Z",
  merge_commit_sha: sha,
  head: { ref: "changeset-release/main", repo: { full_name: repository } },
  base: { ref: "main", repo: { full_name: repository } },
};

test("only the exact merged same-repository version PR selects a release", () => {
  expect(isVersionMerge(pr, repository, sha)).toBe(true);
  expect(isVersionMerge({ ...pr, merged_at: null }, repository, sha)).toBe(false);
  expect(isVersionMerge(pr, repository, "b".repeat(40))).toBe(false);
  expect(isVersionMerge({ ...pr, head: { ...pr.head, ref: "feature" } }, repository, sha)).toBe(
    false,
  );
  expect(
    isVersionMerge(
      { ...pr, head: { ...pr.head, repo: { full_name: "other/lenso" } } },
      repository,
      sha,
    ),
  ).toBe(false);
  expect(isVersionMerge({ ...pr, base: { ...pr.base, ref: "develop" } }, repository, sha)).toBe(
    false,
  );
});

test("select stable public version increases, not dependency-only edits", () => {
  const before = { name: "@lenso/core", version: "0.3.0" };
  expect(changedPublicVersion(before, { ...before, version: "0.3.1" })).toBe(true);
  expect(changedPublicVersion(before, before)).toBe(false);
  expect(changedPublicVersion(before, { ...before, version: "0.3.1", private: true })).toBe(false);
  expect(changedPublicVersion(null, before)).toBe(true);
  expect(() => changedPublicVersion(before, { ...before, version: "0.2.0" })).toThrow("decreased");
  expect(() => changedPublicVersion(before, { ...before, version: "0.4.0-beta.1" })).toThrow(
    "stable",
  );
  expect(() =>
    changedPublicVersion(before, { ...before, name: "@lenso/other", version: "0.4.0" }),
  ).toThrow("change");
});
