import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  assertPublishContext,
  assertRepository,
  existingMatches,
  policyFrom,
  prepare,
  publish,
  releaseSet,
  sha256,
  validateReceipt,
  type Receipt,
} from "./release-ci";

const policy = { registry: "https://registry.npmjs.org", access: "public", tag: "next" } as const;
const env = {
  RELEASE_REGISTRY: policy.registry,
  RELEASE_ACCESS: policy.access,
  RELEASE_TAG: policy.tag,
  RELEASE_PACKAGES: "fixture",
  GITHUB_SHA: "a".repeat(40),
  GITHUB_REPOSITORY: "fixture/repo",
  GITHUB_RUN_ID: "123",
  GITHUB_RUN_ATTEMPT: "1",
};
const bytes = new TextEncoder().encode("verified archive fixture");
const pkg = {
  name: "fixture",
  version: "1.0.0",
  archive: "fixture-1.0.0.tgz",
  sha256: sha256(bytes),
};
const receipt: Receipt = {
  schemaVersion: 2,
  sourceSha: env.GITHUB_SHA,
  repository: env.GITHUB_REPOSITORY,
  runId: env.GITHUB_RUN_ID,
  runAttempt: "1",
  policy,
  packages: [pkg],
};

test("policy has no registry/access/tag defaults and rejects unsupported registry", () => {
  expect(policyFrom(env)).toEqual(policy);
  for (const key of ["RELEASE_REGISTRY", "RELEASE_ACCESS", "RELEASE_TAG"])
    expect(() => policyFrom({ ...env, [key]: "" })).toThrow("required");
  expect(() => policyFrom({ ...env, RELEASE_REGISTRY: "https://other.invalid" })).toThrow(
    "OIDC registry",
  );
  expect(() => policyFrom({ ...env, RELEASE_ACCESS: "guess" })).toThrow("Access");
  expect(() => policyFrom({ ...env, RELEASE_TAG: "--evil" })).toThrow("dist-tag");
  for (const tag of ["x", "v1", "v1.x", "v1.2.3", "v1.2.3-beta"])
    expect(() => policyFrom({ ...env, RELEASE_TAG: tag })).toThrow("dist-tag");
  for (const tag of ["latest", "next", "beta", "vnext"])
    expect(policyFrom({ ...env, RELEASE_TAG: tag }).tag).toBe(tag);
  expect(releaseSet("@lenso/core, @lenso/auth")).toEqual(["@lenso/core", "@lenso/auth"]);
  for (const value of [undefined, "", "@lenso/core,@lenso/core", "@lenso/core,", "*"])
    expect(() => releaseSet(value)).toThrow("explicitly");
  assertRepository({ type: "git", url: "git+https://github.com/fixture/repo.git" }, "fixture/repo");
  for (const value of [undefined, { url: "https://github.com/another/repo" }])
    expect(() => assertRepository(value, "fixture/repo")).toThrow("repository.url");
});

test("receipts bind source/run/policy, allow same-run retry and reject path escape/duplicates", () => {
  validateReceipt(receipt, env);
  validateReceipt(receipt, { ...env, GITHUB_RUN_ATTEMPT: "2" });
  for (const key of ["GITHUB_SHA", "GITHUB_REPOSITORY", "GITHUB_RUN_ID"])
    expect(() => validateReceipt(receipt, { ...env, [key]: "wrong" })).toThrow("source/run");
  expect(() => validateReceipt({ ...receipt, runAttempt: "2" }, env)).toThrow("source/run");
  expect(() => validateReceipt(receipt, { ...env, RELEASE_TAG: "latest" })).toThrow(
    "policy changed",
  );
  for (const archive of ["../escape.tgz", "/escape.tgz", "x\\escape.tgz", "-bad.tgz"])
    expect(() => validateReceipt({ ...receipt, packages: [{ ...pkg, archive }] }, env)).toThrow(
      "Invalid",
    );
  expect(() => validateReceipt({ ...receipt, packages: [pkg, pkg] }, env)).toThrow("duplicate");
  expect(() => validateReceipt({ ...receipt, packages: [] }, env)).toThrow("Empty");
  expect(() => validateReceipt(receipt, { ...env, RELEASE_PACKAGES: "different" })).toThrow(
    "release set",
  );
  expect(() =>
    validateReceipt(
      {
        ...receipt,
        policy: { ...policy, tag: "latest" },
        packages: [{ ...pkg, version: "1.0.0-beta.1" }],
      },
      { ...env, RELEASE_TAG: "latest" },
    ),
  ).toThrow("Prereleases");
});

test("local/default publish is refused before reading archives or invoking npm", async () => {
  expect(() => assertPublishContext({})).toThrow("protected");
  const child = Bun.spawn(
    [process.execPath, `${import.meta.dir}/release-ci.ts`, "publish", "does-not-exist.json"],
    {
      env: { PATH: process.env.PATH },
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  expect(await child.exited).toBe(1);
  expect(await new Response(child.stderr).text()).toContain("protected GitHub Actions");
  expect(await new Response(child.stdout).text()).toBe("");
  const ci = {
    GITHUB_ACTIONS: "true",
    GITHUB_EVENT_NAME: "workflow_dispatch",
    GITHUB_REF: "refs/heads/main",
    RELEASE_PROTECTED_JOB: "npm-release",
    ACTIONS_ID_TOKEN_REQUEST_URL: "fixture",
    ACTIONS_ID_TOKEN_REQUEST_TOKEN: "fixture",
  };
  assertPublishContext(ci);
  expect(() => assertPublishContext({ ...ci, NODE_AUTH_TOKEN: "fixture" })).toThrow("fallback");
  expect(() => assertPublishContext({ ...ci, GITHUB_EVENT_NAME: "pull_request" })).toThrow(
    "protected",
  );
});

function registry(status: number, archive = bytes, origin: string = policy.registry) {
  const urls: string[] = [];
  const request = (async (url: string | URL | Request) => {
    urls.push(String(url));
    return urls.length === 1
      ? new Response(
          JSON.stringify({
            name: pkg.name,
            version: pkg.version,
            dist: { tarball: `${origin}/fixture.tgz` },
          }),
          { status },
        )
      : new Response(archive);
  }) as typeof fetch;
  return { request, urls };
}

test("safe retry compares existing archive bytes, not version existence alone", async () => {
  const match = registry(200);
  expect(await existingMatches(pkg, policy, match.request)).toBe(true);
  expect(match.urls).toHaveLength(2);
  expect(await existingMatches(pkg, policy, registry(404).request)).toBe(false);
  await expect(existingMatches(pkg, policy, registry(403).request)).rejects.toThrow(
    "refusing publish",
  );
  await expect(
    existingMatches(pkg, policy, registry(200, new TextEncoder().encode("different")).request),
  ).rejects.toThrow("differs");
  const redirected = registry(200, bytes, "https://untrusted.invalid");
  await expect(existingMatches(pkg, policy, redirected.request)).rejects.toThrow("origin");
  expect(redirected.urls).toHaveLength(1);
});

test("workflow boundaries: only protected publish gets OIDC, actions are immutable", async () => {
  for (const file of ["checks", "version", "release"]) {
    const text = await Bun.file(`${import.meta.dir}/../.github/workflows/${file}.yml`).text();
    const actions = [...text.matchAll(/uses: ([^\s]+)/g)].map((match) => match[1]!);
    expect(actions.length).toBeGreaterThan(0);
    for (const action of actions) expect(action).toMatch(/@[a-f0-9]{40}$/);
    if (file !== "release") expect(text).not.toContain("id-token:");
    if (file === "checks") {
      expect(text).toContain("pull_request:");
      expect(text).not.toContain("pull_request_target");
      expect(text).not.toContain(": write");
      expect(text).toContain("persist-credentials: false");
    }
    if (file === "version") expect(text).not.toMatch(/^\s+publish:/m);
    if (file === "release") {
      const [prepareJob, publishJob] = text.split("\n  publish:");
      expect(prepareJob).not.toContain("id-token:");
      expect(publishJob).toContain("environment: npm-release");
      expect(publishJob).toContain("id-token: write");
      expect(publishJob).not.toMatch(/bun (?:install|run build|pm pack)/);
      expect(publishJob).toContain("needs.prepare.outputs.artifact_name");
    }
  }
});

async function batch(
  run: (fixture: {
    execute: () => Promise<void>;
    writes: string[][];
    published: Map<string, Uint8Array>;
    directory: string;
    consumer: typeof pkg;
  }) => Promise<void>,
  options: {
    reversed?: boolean;
    fail?: boolean;
    conflict?: boolean;
    publishConfig?: Record<string, unknown>;
  } = {},
) {
  const directory = await mkdtemp(join(tmpdir(), "lenso-publish-fixture-"));
  const consumer = { ...pkg, name: "consumer", archive: "consumer-1.0.0.tgz" };
  const packages = options.reversed ? [consumer, pkg] : [pkg, consumer];
  const writes: string[][] = [];
  const published = new Map<string, Uint8Array>();
  if (options.conflict) published.set(consumer.name, new TextEncoder().encode("different"));
  const path = join(directory, "release.json");
  try {
    await Bun.write(path, JSON.stringify({ ...receipt, packages }));
    for (const archive of packages) await Bun.write(join(directory, archive.archive), bytes);
    const request = (async (input: string | URL | Request) => {
      const url = new URL(String(input));
      if (url.pathname.endsWith(".tgz"))
        return new Response(new Uint8Array(published.get(url.pathname.slice(1, -4))!));
      const name = decodeURIComponent(url.pathname.split("/")[1]!);
      return published.has(name)
        ? new Response(
            JSON.stringify({
              name,
              version: "1.0.0",
              dist: { tarball: `${policy.registry}/${name}.tgz` },
            }),
          )
        : new Response(null, { status: 404 });
    }) as typeof fetch;
    const command = async (args: string[]) => {
      if (args[0] === "tar") {
        const archive = packages.find((item) => join(directory, item.archive) === args[2])!;
        return JSON.stringify({
          name: archive.name,
          version: archive.version,
          repository: { type: "git", url: `git+https://github.com/${env.GITHUB_REPOSITORY}.git` },
          dependencies: archive.name === consumer.name ? { fixture: "^1.0.0" } : {},
          publishConfig: options.publishConfig,
        });
      }
      writes.push(args);
      if (options.fail) throw Error("fixture publication failed");
      const archive = packages.find((item) => join(directory, item.archive) === args[2])!;
      published.set(archive.name, new Uint8Array(await Bun.file(args[2]!).arrayBuffer()));
      return "";
    };
    const execute = () =>
      publish(
        path,
        {
          ...env,
          RELEASE_PACKAGES: "fixture,consumer",
          GITHUB_ACTIONS: "true",
          GITHUB_EVENT_NAME: "workflow_dispatch",
          GITHUB_REF: "refs/heads/main",
          RELEASE_PROTECTED_JOB: "npm-release",
          ACTIONS_ID_TOKEN_REQUEST_URL: "fixture",
          ACTIONS_ID_TOKEN_REQUEST_TOKEN: "fixture",
        },
        { command, request },
      );
    await run({ execute, writes, published, directory, consumer });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

test("publisher uses the same archives in dependency order, disables scripts and safely retries", async () => {
  await batch(async ({ execute, writes, directory, consumer }) => {
    await execute();
    expect(writes.map((args) => args.slice(0, 3))).toEqual([
      ["npm", "publish", join(directory, pkg.archive)],
      ["npm", "publish", join(directory, consumer.archive)],
    ]);
    for (const args of writes)
      expect(args.slice(3)).toEqual([
        "--ignore-scripts",
        "--registry",
        policy.registry,
        "--access",
        "public",
        "--tag",
        "next",
      ]);
    await execute();
    expect(writes).toHaveLength(2);
  });
});

test("publisher validates the entire batch before writes and stops on first write failure", async () => {
  await batch(async ({ execute, writes, directory, consumer }) => {
    await Bun.write(join(directory, consumer.archive), "tampered");
    await expect(execute()).rejects.toThrow("SHA-256");
    expect(writes).toHaveLength(0);
  });
  for (const options of [
    { reversed: true },
    { conflict: true },
    { publishConfig: { registry: "https://evil.invalid" } },
  ]) {
    await batch(async ({ execute, writes }) => {
      await expect(execute()).rejects.toThrow();
      expect(writes).toHaveLength(0);
    }, options);
  }
  await batch(
    async ({ execute, writes }) => {
      await expect(execute()).rejects.toThrow("fixture publication failed");
      expect(writes).toHaveLength(1);
      expect(writes[0]![0]).toBe("npm");
    },
    { fail: true },
  );
});

test("real preparation records the checked-out source/run and explicit archive subset", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-ci-prepare-fixture-"));
  async function command(args: string[]) {
    const child = Bun.spawn(args, {
      cwd: directory,
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    });
    const [code, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    expect(code, stderr).toBe(0);
    return stdout.trim();
  }
  try {
    await Bun.write(join(directory, ".gitignore"), "output/\nnode_modules/\nbun.lock\n");
    await Bun.write(
      join(directory, "package.json"),
      JSON.stringify({
        private: true,
        workspaces: ["packages/*"],
        engines: { bun: Bun.version },
      }),
    );
    for (const name of ["fixture", "unselected"]) {
      await Bun.write(
        join(directory, `packages/${name}/package.json`),
        JSON.stringify({
          name,
          version: "1.0.0",
          files: ["dist"],
          exports: "./dist/index.js",
          scripts: { build: "bun -e 'process.exit(0)'" },
        }),
      );
      await Bun.write(
        join(directory, `packages/${name}/dist/index.js`),
        "export const fixture = true;\n",
      );
    }
    await command(["git", "init", "--initial-branch=main"]);
    await command(["git", "add", "."]);
    await command([
      "env",
      "GIT_EDITOR=true",
      "git",
      "-c",
      "user.name=Fixture",
      "-c",
      "user.email=fixture@example.invalid",
      "-c",
      "commit.gpgsign=false",
      "commit",
      "-m",
      "Fixture",
    ]);
    const sourceSha = await command(["git", "rev-parse", "HEAD"]);
    const path = await prepare(directory, { ...env, GITHUB_SHA: sourceSha });
    const data = await Bun.file(path).json();
    expect(data.sourceSha).toBe(sourceSha);
    expect(data.runId).toBe(env.GITHUB_RUN_ID);
    expect(data.policy).toEqual(policy);
    expect(data.packages.map((item: { name: string }) => item.name)).toEqual(["fixture"]);
    const archive = join(
      directory,
      "output",
      "release",
      path.split("/").at(-2)!,
      data.packages[0].archive,
    );
    expect(sha256(new Uint8Array(await Bun.file(archive).arrayBuffer()))).toBe(
      data.packages[0].sha256,
    );
    await expect(prepare(directory, { ...env, GITHUB_SHA: "b".repeat(40) })).rejects.toThrow(
      "Checkout SHA",
    );
    await Bun.write(join(directory, "untracked.ts"), "export const changed = true;\n");
    await expect(prepare(directory, { ...env, GITHUB_SHA: sourceSha })).rejects.toThrow(
      "unchanged source",
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
