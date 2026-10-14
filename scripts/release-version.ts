import { applyReleasePlan } from "@changesets/apply-release-plan";
import { assembleReleasePlan } from "@changesets/assemble-release-plan";
import { readConfig } from "@changesets/config";
import { readChangesets } from "@changesets/read";
import { getPackages } from "@manypkg/get-packages";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// The staged SDK is an immutable foreign build input, not a Lenso release package.
export async function prepareVersions(
  root: string,
  options: { changesets?: string[]; write?: boolean } = {},
) {
  root = resolve(root);
  const discovered = await getPackages(root);
  if (resolve(discovered.rootDir) !== root) throw Error("Release root must be the workspace root");
  const packages = {
    ...discovered,
    packages: discovered.packages.filter(
      (pkg) => resolve(pkg.dir) !== join(root, ".runtime-deps/sdk"),
    ),
  };
  const result = await readConfig(root, packages);
  for (const warning of result.warnings) console.warn(warning);
  if (result.errors) throw Error(result.errors.join("\n"));
  const config = result.config;
  if (
    config.commit ||
    config.ignore.length ||
    (await Bun.file(join(root, ".changeset/pre.json")).exists())
  ) {
    throw Error("Release preparation requires stable, non-committing, unignored Changesets");
  }
  const pending = await readChangesets(root);
  const ids = options.changesets;
  if (ids) {
    if (!ids.length || new Set(ids).size !== ids.length)
      throw Error("Select distinct pending changeset IDs");
    for (const id of ids) {
      if (!pending.some((changeset) => changeset.id === id))
        throw Error(`Unknown pending changeset: ${id}`);
    }
  }
  const selected = ids ? pending.filter((changeset) => ids.includes(changeset.id)) : pending;
  const owned = new Set(packages.packages.map((pkg) => pkg.packageJson.name));
  for (const changeset of selected) {
    for (const release of changeset.releases) {
      if (!owned.has(release.name))
        throw Error(`Changeset ${changeset.id} targets a foreign package: ${release.name}`);
    }
  }
  const plan = assembleReleasePlan(selected, packages, config, undefined);
  if (options.write) {
    if (!selected.length) throw Error("No pending changesets selected");
    const context = dirname(fileURLToPath(import.meta.resolve("@changesets/cli/package.json")));
    await applyReleasePlan(plan, packages, config, undefined, context);
  }
  return plan;
}

if (import.meta.main) {
  try {
    const [command, ...args] = process.argv.slice(2);
    if (command !== "status" && command !== "version") throw Error("Expected status or version");
    const changesets: string[] = [];
    let output: string | undefined;
    for (let i = 0; i < args.length; i += 2) {
      const value = args[i + 1];
      if (!value || value.startsWith("--")) throw Error("Expected option value");
      if (args[i] === "--changeset") changesets.push(value);
      else if (args[i] === "--output" && command === "status" && !output) output = value;
      else throw Error(`Unsupported option: ${args[i]}`);
    }
    const plan = await prepareVersions(resolve(import.meta.dir, ".."), {
      ...(changesets.length ? { changesets } : {}),
      write: command === "version",
    });
    const json = `${JSON.stringify(plan, null, 2)}\n`;
    if (output) await Bun.write(output, json);
    else console.log(json.trimEnd());
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
