import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { validateArchive, type Manifest } from "./release-verify";

// One reviewed local combination, not a general publisher or release registry.
const directories = ["lenso", "engine", "cli", "manage", "mcp", "auth", "web"] as const;
const groups = [
  "dependencies",
  "optionalDependencies",
  "peerDependencies",
  "devDependencies",
] as const;

export function combinationManifest(
  manifest: Manifest,
  versions: ReadonlyMap<string, string>,
): Manifest {
  const result = { ...manifest };
  for (const group of groups) {
    if (!manifest[group]) continue;
    result[group] = Object.fromEntries(
      Object.entries(manifest[group]!).map(([name, value]) => {
        const version = versions.get(name);
        if (version) return [name, version];
        if (/^(workspace:|link:|file:)/.test(value))
          throw Error(`Unresolved local dependency: ${name}`);
        return [name, value];
      }),
    );
  }
  return result;
}

async function run(cwd: string, argv: string[]): Promise<string> {
  const process = Bun.spawn(argv, {
    cwd,
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
    env: { ...globalThis.process.env, COPYFILE_DISABLE: "1" },
  });
  const [exit, out, error] = await Promise.all([
    process.exited,
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
  ]);
  if (exit !== 0) throw Error(`Local combination command failed: ${argv[0]} (${exit})\n${error}`);
  return out;
}

export async function packDevCombination(root: string, output: string) {
  const workspace = await Bun.file(join(root, "package.json")).json();
  if (Bun.version !== workspace.engines.bun) throw Error(`Use Bun ${workspace.engines.bun}.`);
  if ((await run(root, ["git", "status", "--porcelain"])).trim())
    throw Error("Commit or isolate changes before creating an immutable combination.");
  const source = (await run(root, ["git", "rev-parse", "HEAD"])).trim();
  const packages = await Promise.all(
    directories.map(async (directory) => {
      const path = join(root, "packages", directory);
      const manifest = (await Bun.file(join(path, "package.json")).json()) as Manifest;
      if (manifest.private || !manifest.name.startsWith("@lenso/"))
        throw Error("Unexpected combination package.");
      return { path, manifest };
    }),
  );
  const versions = new Map(packages.map((pkg) => [pkg.manifest.name, pkg.manifest.version]));
  // Do not replace existing artifacts, even when this command previously failed.
  await mkdir(output);
  const temporary = await mkdtemp(join(tmpdir(), "lenso-combination-"));
  try {
    const records = [];
    for (const pkg of packages) {
      const label = pkg.manifest.name.replace("@lenso/", "lenso-");
      const raw = join(temporary, `${label}-raw.tgz`);
      await run(pkg.path, [process.execPath, "pm", "pack", "--ignore-scripts", "--filename", raw]);
      const entries = (await run(root, ["tar", "-tf", raw])).trim().split("\n");
      const original = JSON.parse(
        await run(root, ["tar", "-xOf", raw, "package/package.json"]),
      ) as Manifest;
      validateArchive(pkg.manifest, original, entries);
      const stage = join(temporary, label);
      await mkdir(stage);
      await run(stage, ["tar", "-xf", raw]);
      const changed: string[] = [];
      async function normalize(directory: string) {
        for (const entry of await readdir(directory, { withFileTypes: true })) {
          const path = join(directory, entry.name);
          if (entry.isDirectory()) await normalize(path);
          else if (entry.isFile() && entry.name === "package.json") {
            const before = JSON.parse(await readFile(path, "utf8")) as Manifest;
            const after = combinationManifest(before, versions);
            if (JSON.stringify(before) !== JSON.stringify(after))
              changed.push(path.slice(stage.length + 1));
            await writeFile(path, JSON.stringify(after, null, 2) + "\n");
          }
        }
      }
      await normalize(join(stage, "package"));
      const packed = (await Bun.file(join(stage, "package/package.json")).json()) as Manifest;
      validateArchive(pkg.manifest, packed, entries);
      const filename = `${label}-${packed.version}.tgz`;
      const archive = join(output, filename);
      await run(stage, ["tar", "-czf", archive, "package"]);
      records.push({
        name: packed.name,
        version: packed.version,
        filename,
        sha256: createHash("sha256")
          .update(await readFile(archive))
          .digest("hex"),
        normalizedManifests: changed,
        dependencies: packed.dependencies ?? {},
        peerDependencies: packed.peerDependencies ?? {},
      });
    }
    const dependencies = Object.fromEntries(
      records.map((pkg) => [pkg.name, `file:${join(output, pkg.filename)}`]),
    );
    const combination = {
      schemaVersion: 1,
      source,
      bun: Bun.version,
      purpose: "local reviewed candidate integration; not publication",
      packageVersionsChanged: false,
      packages: records,
      singleInstance: {
        packages: [...versions.keys()],
        rule: "Use this supplied dependency/override set together; do not mix same-version registry and candidate identities.",
      },
      dependencySources: {
        candidates: "these SHA256-identified local archives",
        otherPackages: "https://registry.npmjs.org (existing declared versions; no new dependency)",
      },
    };
    await writeFile(join(output, "combination.json"), JSON.stringify(combination, null, 2) + "\n");
    await writeFile(
      join(output, "install-fragment.json"),
      JSON.stringify({ dependencies, overrides: dependencies }, null, 2) + "\n",
    );
    await writeFile(
      join(output, "README.txt"),
      `Local combination from ${source}\n\nArchives are already packed and metadata-normalized by their TS owner. Do not re-pack or edit installed package metadata. Merge install-fragment.json into a disposable consumer's manifest, preserving unrelated dependencies, then run bun install. Its file paths select this immutable directory; moving the directory requires a new upstream-generated fragment. Keep the whole override set to prevent Bun from resolving same-version registry packages beside these candidates. This is not a published release or a no-override registry-consumer claim. Ordinary TS services require no Runtime. Check consumer verification evidence separately; packing alone is not verification.\n`,
    );
    const sums = [];
    for (const filename of (await readdir(output)).sort())
      sums.push(
        `${createHash("sha256")
          .update(await readFile(join(output, filename)))
          .digest("hex")}  ${filename}`,
      );
    await writeFile(join(output, "SHA256SUMS"), sums.join("\n") + "\n");
    return combination;
  } catch (error) {
    await rm(output, { recursive: true, force: true });
    throw error;
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

if (import.meta.main) {
  if (process.argv.length !== 3)
    throw Error("Usage: bun scripts/pack-dev-combination.ts NEW_OUTPUT_DIRECTORY");
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  console.log(JSON.stringify(await packDevCombination(root, resolve(process.argv[2]!))));
}
