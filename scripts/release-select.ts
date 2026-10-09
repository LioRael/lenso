import { appendFile } from "node:fs/promises";
import { releaseSet } from "./release-ci";

interface VersionPR {
  merged_at: string | null;
  merge_commit_sha: string;
  head: { ref: string; repo: { full_name: string } | null };
  base: { ref: string; repo: { full_name: string } };
}

export function isVersionMerge(pr: VersionPR, repository: string, sha: string) {
  return (
    !!pr.merged_at &&
    pr.merge_commit_sha === sha &&
    pr.head.ref === "changeset-release/main" &&
    pr.head.repo?.full_name === repository &&
    pr.base.ref === "main" &&
    pr.base.repo.full_name === repository
  );
}

export function changedPublicVersion(
  before: { name: string; version: string; private?: boolean } | null,
  after: { name: string; version: string; private?: boolean },
) {
  if (after.private || before?.version === after.version) return false;
  releaseSet(after.name);
  if (!Bun.semver.satisfies(after.version, "*") || after.version.includes("-"))
    throw Error(`Invalid stable version: ${after.name}@${after.version}`);
  if (before && (before.name !== after.name || !Bun.semver.order(after.version, before.version)))
    throw Error(`Invalid version change: ${after.name}`);
  if (before && Bun.semver.order(after.version, before.version) < 0)
    throw Error(`Version decreased: ${after.name}`);
  return true;
}

function git(...args: string[]) {
  const result = Bun.spawnSync(["git", ...args]);
  if (result.exitCode) throw Error(result.stderr.toString());
  return result.stdout.toString();
}

if (import.meta.main) {
  const env = process.env;
  let names: string[] = [];
  if (env.GITHUB_EVENT_NAME === "workflow_dispatch") names = releaseSet(env.RELEASE_PACKAGES);
  else if (env.GITHUB_EVENT_NAME === "push" && env.GITHUB_REF === "refs/heads/main") {
    const sha = env.GITHUB_SHA!,
      repository = env.GITHUB_REPOSITORY!;
    if (!/^[a-f0-9]{40}$/.test(sha) || !/^[\w.-]+\/[\w.-]+$/.test(repository))
      throw Error("Invalid release source");
    const response = await fetch(
      `https://api.github.com/repos/${repository}/commits/${sha}/pulls?per_page=100`,
      {
        headers: {
          Authorization: `Bearer ${env.GITHUB_TOKEN}`,
          Accept: "application/vnd.github+json",
        },
      },
    );
    if (!response.ok) throw Error(`Version PR lookup failed: ${response.status}`);
    const prs = (await response.json()) as VersionPR[];
    if (prs.some((pr) => isVersionMerge(pr, repository, sha))) {
      const base = git("rev-parse", `${sha}^1`).trim();
      const paths = git("diff", "--name-only", base, sha, "--", "packages")
        .trim()
        .split("\n")
        .filter((path) => /^packages\/[^/]+\/package.json$/.test(path));
      for (const path of paths) {
        const after = JSON.parse(git("show", `${sha}:${path}`));
        const old = Bun.spawnSync(["git", "show", `${base}:${path}`]);
        const before = old.exitCode ? null : JSON.parse(old.stdout.toString());
        if (changedPublicVersion(before, after)) names.push(after.name);
      }
      if (!names.length) throw Error("Merged version PR has no stable public version changes");
      names = releaseSet(names.join(","));
    }
  } else throw Error("Unsupported release event");
  console.log(
    names.length ? `Selected: ${names.join(",")}` : "No merged version PR; skipping release",
  );
  await appendFile(env.GITHUB_OUTPUT!, `packages=${names.join(",")}\n`);
}
