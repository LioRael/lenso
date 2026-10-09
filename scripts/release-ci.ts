import { createHash } from "node:crypto";
import { basename, dirname, join, resolve } from "node:path";
import { verifyRelease } from "./release-verify";

export interface Policy {
  registry: string;
  access: "public" | "restricted";
  tag: string;
}
export interface Archive {
  name: string;
  version: string;
  archive: string;
  sha256: string;
}
export interface Receipt {
  schemaVersion: 2;
  sourceSha: string;
  repository: string;
  runId: string;
  runAttempt: string;
  policy: Policy;
  packages: Archive[];
}

export function policyFrom(env: Record<string, string | undefined>): Policy {
  const registry = env.RELEASE_REGISTRY;
  const access = env.RELEASE_ACCESS;
  const tag = env.RELEASE_TAG;
  if (!registry || !access || !tag)
    throw Error("RELEASE_REGISTRY, RELEASE_ACCESS and RELEASE_TAG are required");
  // npm trusted publishing authenticates to npm, not arbitrary registry hosts.
  if (registry !== "https://registry.npmjs.org")
    throw Error(
      "Only the explicitly approved npm OIDC registry https://registry.npmjs.org is supported",
    );
  if (access !== "public" && access !== "restricted")
    throw Error("Access must be public or restricted");
  if (
    !/^[a-z][a-z0-9._-]*$/.test(tag) ||
    tag === "x" ||
    /^v\d+(?:\.(?:\d+|x)){0,2}(?:-[a-z0-9.-]+)?$/.test(tag)
  )
    throw Error("Invalid explicit dist-tag; use a named channel, not a version range");
  return { registry, access, tag };
}

export function assertPublishContext(env: Record<string, string | undefined>) {
  if (
    env.GITHUB_ACTIONS !== "true" ||
    !["workflow_dispatch", "push"].includes(env.GITHUB_EVENT_NAME ?? "") ||
    env.GITHUB_REF !== "refs/heads/main" ||
    env.RELEASE_PROTECTED_JOB !== "npm-release" ||
    !env.ACTIONS_ID_TOKEN_REQUEST_URL ||
    !env.ACTIONS_ID_TOKEN_REQUEST_TOKEN
  )
    throw Error("Publishing requires the explicitly invoked protected GitHub Actions job on main");
  if (env.NODE_AUTH_TOKEN || env.NPM_TOKEN) throw Error("Token fallback is forbidden; use OIDC");
}

export function releaseSet(value: string | undefined): string[] {
  const names = (value ?? "").split(",").map((name) => name.trim());
  if (
    names.some((name) => !/^(?:@[a-z0-9._-]+\/)?[a-z0-9][a-z0-9._-]*$/.test(name)) ||
    new Set(names).size !== names.length
  )
    throw Error(
      "RELEASE_PACKAGES must explicitly list unique comma-separated public package names",
    );
  return names;
}

export function validateReceipt(receipt: Receipt, env: Record<string, string | undefined>) {
  if (
    receipt.schemaVersion !== 2 ||
    !/^[a-f0-9]{40}$/.test(receipt.sourceSha) ||
    receipt.sourceSha !== env.GITHUB_SHA ||
    receipt.repository !== env.GITHUB_REPOSITORY ||
    receipt.runId !== env.GITHUB_RUN_ID ||
    !/^[1-9]\d*$/.test(receipt.runAttempt) ||
    !/^[1-9]\d*$/.test(env.GITHUB_RUN_ATTEMPT ?? "") ||
    Number(receipt.runAttempt) > Number(env.GITHUB_RUN_ATTEMPT)
  )
    throw Error("Receipt source/run does not match this workflow");
  const policy = policyFrom(env);
  if (JSON.stringify(receipt.policy) !== JSON.stringify(policy))
    throw Error("Receipt policy changed");
  if (!Array.isArray(receipt.packages) || !receipt.packages.length)
    throw Error("Empty release set");
  const names = new Set<string>();
  for (const pkg of receipt.packages) {
    if (
      !/^(?:@[a-z0-9._-]+\/)?[a-z0-9][a-z0-9._-]*$/.test(pkg.name) ||
      !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(pkg.version) ||
      basename(pkg.archive) !== pkg.archive ||
      !/^[a-z0-9][a-z0-9._-]*\.tgz$/.test(pkg.archive) ||
      !/^[a-f0-9]{64}$/.test(pkg.sha256) ||
      names.has(pkg.name)
    )
      throw Error("Invalid or duplicate receipt package");
    if (pkg.version.includes("-") && policy.tag === "latest")
      throw Error("Prereleases cannot use latest");
    names.add(pkg.name);
  }
  const requested = releaseSet(env.RELEASE_PACKAGES);
  if (names.size !== requested.length || requested.some((name) => !names.has(name)))
    throw Error("Receipt release set differs from dispatch");
}

export function sha256(bytes: Uint8Array) {
  return createHash("sha256").update(bytes).digest("hex");
}

export function assertRepository(value: unknown, repository: string) {
  const url = value && typeof value === "object" && "url" in value ? value.url : value;
  const expected = `https://github.com/${repository}`;
  if (
    typeof url !== "string" ||
    ![expected, `${expected}.git`, `git+${expected}.git`].includes(url)
  )
    throw Error(
      "Package repository.url must identify this GitHub repository before npm OIDC publication",
    );
}

export async function existingMatches(
  pkg: Archive,
  policy: Policy,
  request = fetch,
): Promise<boolean> {
  const response = await request(
    `${policy.registry}/${encodeURIComponent(pkg.name)}/${pkg.version}`,
  );
  if (response.status === 404) return false;
  if (!response.ok)
    throw Error(`${pkg.name}: registry read failed (${response.status}); refusing publish`);
  const metadata = (await response.json()) as {
    name?: string;
    version?: string;
    dist?: { tarball?: string };
  };
  if (metadata.name !== pkg.name || metadata.version !== pkg.version || !metadata.dist?.tarball)
    throw Error(`${pkg.name}: invalid registry metadata`);
  const url = new URL(metadata.dist.tarball);
  if (url.origin !== policy.registry || url.username || url.password)
    throw Error("Unexpected registry tarball origin");
  const archive = await request(url);
  if (!archive.ok) throw Error(`${pkg.name}: existing archive download failed`);
  if (sha256(new Uint8Array(await archive.arrayBuffer())) !== pkg.sha256)
    throw Error(
      `${pkg.name}@${pkg.version}: registry archive differs; never overwrite or unpublish`,
    );
  return true;
}

async function command(args: string[]): Promise<string> {
  const child = Bun.spawn(args, { stdin: "ignore", stdout: "pipe", stderr: "pipe" });
  const [code, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  if (code !== 0) throw Error(`${args[0]} failed (${code})\n${stdout}\n${stderr}`);
  return stdout;
}

export async function waitForVisibility(
  packages: Archive[],
  policy: Policy,
  request = fetch,
  clock = { now: Date.now, sleep: (milliseconds: number) => Bun.sleep(milliseconds) },
  timeoutMs = 20 * 60 * 1000,
) {
  const deadline = clock.now() + timeoutMs;
  let pending = packages;
  while (pending.length) {
    const remaining: Archive[] = [];
    for (const pkg of pending) {
      if (await existingMatches(pkg, policy, request))
        console.log(`Published and matched: ${pkg.name}@${pkg.version}`);
      else remaining.push(pkg);
    }
    pending = remaining;
    if (!pending.length) return;
    const remainingMs = deadline - clock.now();
    const names = pending.map((pkg) => `${pkg.name}@${pkg.version}`).join(", ");
    if (remainingMs <= 0)
      throw Error(`Submitted versions are not visible: ${names}; inspect registry before retry`);
    console.log(`Waiting for npm processing: ${names}`);
    await clock.sleep(Math.min(30_000, remainingMs));
  }
}

async function assertSource(root: string, sha: string) {
  if ((await command(["git", "-C", root, "rev-parse", "HEAD"])).trim() !== sha)
    throw Error("Checkout SHA does not match CI source SHA");
  if (
    (await command(["git", "-C", root, "status", "--porcelain", "--untracked-files=normal"])).trim()
  )
    throw Error("Release preparation requires an unchanged source checkout");
}

export async function prepare(root: string, env = process.env) {
  const policy = policyFrom(env);
  const selected = releaseSet(env.RELEASE_PACKAGES);
  if (!env.GITHUB_SHA || !env.GITHUB_REPOSITORY || !env.GITHUB_RUN_ID || !env.GITHUB_RUN_ATTEMPT)
    throw Error("CI source/run metadata is required");
  await assertSource(root, env.GITHUB_SHA);
  const verified = await verifyRelease(root);
  await assertSource(root, env.GITHUB_SHA);
  const report = (await Bun.file(verified).json()) as { packages: Archive[] };
  const packages = report.packages.filter((pkg) => selected.includes(pkg.name));
  if (packages.length !== selected.length) throw Error("Unknown or private release package");
  const receipt: Receipt = {
    schemaVersion: 2,
    sourceSha: env.GITHUB_SHA,
    repository: env.GITHUB_REPOSITORY,
    runId: env.GITHUB_RUN_ID,
    runAttempt: env.GITHUB_RUN_ATTEMPT,
    policy,
    packages: packages.map((pkg) => ({ ...pkg, archive: basename(pkg.archive) })),
  };
  validateReceipt(receipt, env);
  const path = join(dirname(verified), "release.json");
  await Bun.write(path, JSON.stringify(receipt, null, 2) + "\n");
  if (env.GITHUB_OUTPUT)
    await Bun.write(
      env.GITHUB_OUTPUT,
      (await Bun.file(env.GITHUB_OUTPUT).text()) +
        `directory=${dirname(verified)}\nartifact=verified-release-${env.GITHUB_RUN_ID}-${env.GITHUB_RUN_ATTEMPT}\n`,
    );
  console.log(
    `Prepared explicit dependency-ordered set: ${packages.map((pkg) => `${pkg.name}@${pkg.version}`).join(", ")}`,
  );
  return path;
}

export async function publish(path: string, env = process.env, io = { command, request: fetch }) {
  assertPublishContext(env);
  const receipt = (await Bun.file(path).json()) as Receipt;
  validateReceipt(receipt, env);
  // Validate the entire batch before the first write to the registry.
  const manifests = new Map<
    string,
    {
      dependencies?: Record<string, string>;
      optionalDependencies?: Record<string, string>;
      peerDependencies?: Record<string, string>;
      publishConfig?: Record<string, unknown>;
    }
  >();
  for (const pkg of receipt.packages) {
    const archive = join(dirname(path), pkg.archive);
    if (sha256(new Uint8Array(await Bun.file(archive).arrayBuffer())) !== pkg.sha256)
      throw Error(`${pkg.name}: SHA-256 mismatch`);
    const manifest = JSON.parse(
      await io.command(["tar", "-xOzf", archive, "package/package.json"]),
    );
    if (manifest.name !== pkg.name || manifest.version !== pkg.version || manifest.private)
      throw Error("Archive identity mismatch");
    assertRepository(manifest.repository, receipt.repository);
    for (const [key, value] of Object.entries(manifest.publishConfig ?? {})) {
      if (receipt.policy[key as keyof Policy] !== value)
        throw Error(`${pkg.name}: unsupported or conflicting publishConfig.${key}`);
    }
    manifests.set(pkg.name, manifest);
  }
  const selected = new Map(receipt.packages.map((pkg) => [pkg.name, pkg]));
  const preceding = new Set<string>();
  for (const pkg of receipt.packages) {
    const manifest = manifests.get(pkg.name)!;
    for (const group of [
      manifest.dependencies,
      manifest.optionalDependencies,
      manifest.peerDependencies,
    ]) {
      for (const [name, range] of Object.entries(group ?? {})) {
        const dependency = selected.get(name);
        if (
          dependency &&
          (!preceding.has(name) || !Bun.semver.satisfies(dependency.version, range))
        )
          throw Error(`${pkg.name}: invalid dependency order/range for ${name}`);
        if (!dependency) {
          const response = await io.request(
            `${receipt.policy.registry}/${encodeURIComponent(name)}`,
          );
          if (!response.ok)
            throw Error(`${pkg.name}: dependency ${name} cannot be verified in registry`);
          const metadata = (await response.json()) as { versions?: Record<string, unknown> };
          if (
            !Object.keys(metadata.versions ?? {}).some((version) =>
              Bun.semver.satisfies(version, range),
            )
          )
            throw Error(`${pkg.name}: no registry version satisfies ${name}@${range}`);
        }
      }
    }
    preceding.add(pkg.name);
  }
  for (const pkg of receipt.packages) await existingMatches(pkg, receipt.policy, io.request);
  for (const pkg of receipt.packages) {
    if (await existingMatches(pkg, receipt.policy, io.request)) {
      console.log(
        `Already published identical archive: ${pkg.name}@${pkg.version}; tag is not changed`,
      );
      continue;
    }
    const archive = join(dirname(path), pkg.archive);
    if (sha256(new Uint8Array(await Bun.file(archive).arrayBuffer())) !== pkg.sha256)
      throw Error("Archive changed before publish");
    await io.command([
      "npm",
      "publish",
      archive,
      "--ignore-scripts",
      "--registry",
      receipt.policy.registry,
      "--access",
      receipt.policy.access,
      "--tag",
      receipt.policy.tag,
    ]);
    console.log(`Submitted: ${pkg.name}@${pkg.version}; awaiting registry verification`);
  }
  await waitForVisibility(receipt.packages, receipt.policy, io.request);
}

if (import.meta.main) {
  try {
    const [mode, path, ...extra] = process.argv.slice(2);
    if (mode === "prepare" && !path) await prepare(resolve(import.meta.dir, ".."));
    else if (mode === "publish" && path && !extra.length) await publish(resolve(path));
    else
      throw Error(
        "Usage: bun scripts/release-ci.ts prepare | publish <release.json> (publish only in protected CI job)",
      );
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
