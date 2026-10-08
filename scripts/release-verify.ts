import { mkdir, readdir } from "node:fs/promises";
import { join, resolve } from "node:path";

export interface Manifest {
  name: string;
  version: string;
  private?: boolean;
  files?: string[];
  exports?: unknown;
  bin?: string | Record<string, string>;
  main?: string;
  module?: string;
  types?: string;
  scripts?: Record<string, string>;
  dependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
}

export interface ReleasePackage {
  directory: string;
  manifest: Manifest;
}

const runtimeGroups = ["dependencies", "optionalDependencies", "peerDependencies"] as const;
const buildGroups = [...runtimeGroups, "devDependencies"] as const;

export function releaseOrder(packages: ReleasePackage[]): ReleasePackage[] {
  const byName = new Map<string, ReleasePackage>();
  for (const pkg of packages) {
    if (byName.has(pkg.manifest.name)) throw Error(`Duplicate package: ${pkg.manifest.name}`);
    byName.set(pkg.manifest.name, pkg);
  }
  const active = new Set<string>();
  const visited = new Set<string>();
  const ordered: ReleasePackage[] = [];
  function visit(pkg: ReleasePackage) {
    const { name } = pkg.manifest;
    if (visited.has(name)) return;
    if (active.has(name)) throw Error(`Package dependency cycle: ${name}`);
    active.add(name);
    for (const group of buildGroups) {
      for (const [dependency, range] of Object.entries(pkg.manifest[group] ?? {})) {
        const target = byName.get(dependency);
        if (range.startsWith("workspace:") && !target) {
          throw Error(`${name}: unresolved workspace dependency ${dependency}`);
        }
        if (target) {
          if (!pkg.manifest.private && target.manifest.private && group !== "devDependencies") {
            throw Error(`${name}: runtime dependency on private package ${dependency}`);
          }
          visit(target);
        }
      }
    }
    active.delete(name);
    visited.add(name);
    ordered.push(pkg);
  }
  for (const pkg of [...packages].sort((a, b) => a.manifest.name.localeCompare(b.manifest.name))) {
    visit(pkg);
  }
  return ordered.filter((pkg) => !pkg.manifest.private);
}

function strings(value: unknown): string[] {
  if (typeof value === "string") return [value];
  if (value && typeof value === "object") return Object.values(value).flatMap(strings);
  return [];
}

export function validateArchive(source: Manifest, packed: Manifest, entries: string[]) {
  if (packed.private || source.name !== packed.name || source.version !== packed.version) {
    throw Error(`${source.name}: packed identity differs from source`);
  }
  const files = entries.filter((entry) => !entry.endsWith("/"));
  if (
    !files.includes("package/package.json") ||
    !files.some((p) => p.startsWith("package/dist/"))
  ) {
    throw Error(`${source.name}: archive needs package.json and built dist`);
  }
  for (const file of files) {
    if (
      !file.startsWith("package/") ||
      file
        .split("/")
        .some(
          (part) =>
            part === ".." ||
            part === "node_modules" ||
            part === ".lenso" ||
            part === ".npmrc" ||
            part === ".env" ||
            part.startsWith(".env."),
        ) ||
      file.endsWith(".tgz")
    )
      throw Error(`${source.name}: forbidden archive entry ${file}`);
  }
  for (const group of runtimeGroups) {
    for (const [dependency, range] of Object.entries(packed[group] ?? {})) {
      if (/^(workspace:|file:|link:)/.test(range)) {
        throw Error(`${source.name}: packed ${group}.${dependency} still uses ${range}`);
      }
    }
  }
  const targets = [
    ...strings(packed.exports),
    ...strings(packed.bin),
    ...strings(packed.main),
    ...strings(packed.module),
    ...strings(packed.types),
  ];
  for (const target of targets) {
    const path = `package/${target.replace(/^\.\//, "")}`;
    // Package export patterns substitute slashes too, unlike filesystem globs.
    const pattern = new RegExp(
      "^" +
        path
          .split("*")
          .map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
          .join(".+") +
        "$",
    );
    if (target.includes("..") || !files.some((file) => pattern.test(file))) {
      throw Error(`${source.name}: missing packed entry point ${target}`);
    }
  }
}

async function run(cwd: string, command: string[]): Promise<string> {
  const child = Bun.spawn(command, { cwd, stdin: "ignore", stdout: "pipe", stderr: "pipe" });
  const [code, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  if (code !== 0) throw Error(`${command.join(" ")} failed (${code})\n${stdout}\n${stderr}`);
  return stdout;
}

export async function verifyRelease(root: string) {
  const workspace = await Bun.file(join(root, "package.json")).json();
  if (Bun.version !== workspace.engines.bun) {
    throw Error(`Use Bun ${workspace.engines.bun}; running ${Bun.version}`);
  }
  const packages: ReleasePackage[] = [];
  for (const entry of await readdir(join(root, "packages"), { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const directory = join(root, "packages", entry.name);
    const manifest = (await Bun.file(join(directory, "package.json")).json()) as Manifest;
    if (!manifest.private) {
      if (
        !manifest.name ||
        !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(manifest.version)
      ) {
        throw Error(`${directory}: package name and semver version required`);
      }
      if (!manifest.files?.includes("dist") || !manifest.scripts?.build) {
        throw Error(`${manifest.name}: explicit dist files allowlist and build script required`);
      }
    }
    packages.push({ directory, manifest });
  }
  const ordered = releaseOrder(packages);
  // A unique directory prevents old successful receipts from surviving a failed run.
  const output = join(root, "output", "release", `${Date.now()}-${crypto.randomUUID()}`);
  await mkdir(output, { recursive: true });
  const report: {
    name: string;
    version: string;
    archive: string;
    sha256: string;
    entries: string[];
  }[] = [];
  for (const pkg of ordered) {
    console.log(`Build and pack ${pkg.manifest.name}@${pkg.manifest.version}`);
    await run(pkg.directory, [process.execPath, "run", "build"]);
    const archive = join(
      output,
      `${pkg.manifest.name.replaceAll("@", "").replaceAll("/", "-")}-${pkg.manifest.version}.tgz`,
    );
    await run(pkg.directory, [
      process.execPath,
      "pm",
      "pack",
      "--ignore-scripts",
      "--filename",
      archive,
    ]);
    const entries = (await run(root, ["tar", "-tzf", archive])).trim().split("\n");
    const packed = JSON.parse(await run(root, ["tar", "-xOzf", archive, "package/package.json"]));
    validateArchive(pkg.manifest, packed, entries);
    for (const bin of strings(packed.bin)) {
      const content = await run(root, [
        "tar",
        "-xOzf",
        archive,
        `package/${bin.replace(/^\.\//, "")}`,
      ]);
      if (!content.startsWith("#!")) throw Error(`${pkg.manifest.name}: bin needs a shebang`);
    }
    const sha256 = new Bun.CryptoHasher("sha256")
      .update(await Bun.file(archive).arrayBuffer())
      .digest("hex");
    report.push({ name: packed.name, version: packed.version, archive, sha256, entries });
  }
  const receipt = join(output, "verified.json");
  await Bun.write(
    receipt,
    JSON.stringify({ schemaVersion: 1, bun: Bun.version, packages: report }, null, 2) + "\n",
  );
  console.log(`Local archive verification passed. Review file lists and hashes: ${receipt}`);
  console.log(
    "Nothing was published. Registry, ownership, access and dist-tag require human confirmation.",
  );
  return receipt;
}

if (import.meta.main) {
  try {
    if (process.argv.length !== 2)
      throw Error(
        "Usage: bun scripts/release-verify.ts (local verification only; no publish flags)",
      );
    await verifyRelease(resolve(import.meta.dir, ".."));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
