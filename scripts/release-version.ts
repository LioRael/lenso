import { applyReleasePlan } from "@changesets/apply-release-plan";
import { assembleReleasePlan } from "@changesets/assemble-release-plan";
import { readConfig } from "@changesets/config";
import { readChangesets } from "@changesets/read";
import { getPackages } from "@manypkg/get-packages";
import { createHash } from "node:crypto";
import { writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// The staged SDK is an immutable foreign build input, not a Lenso release package.
export async function prepareVersions(
  root: string,
  options: { changesets?: string[]; packages?: string[]; write?: boolean } = {},
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
  const scope = options.packages;
  let deferred: typeof plan.releases = [];
  let deferredChangeset: string | undefined;
  let deferredContent: string | undefined;
  if (scope) {
    if (!ids || !scope.length || new Set(scope).size !== scope.length)
      throw Error("Package scope requires explicit changesets and distinct package names");
    if (config.fixed.length || config.linked.length)
      throw Error("Scoped preparation does not split fixed or linked release groups");
    for (const name of scope) {
      const pkg = packages.packages.find((entry) => entry.packageJson.name === name);
      if (!pkg || pkg.packageJson.private || !plan.releases.some((r) => r.name === name))
        throw Error(`Scope must name a planned public package: ${name}`);
    }
    for (const changeset of selected) {
      if (changeset.releases.some((release) => !scope.includes(release.name)))
        throw Error(`Scope cannot discard an explicit release in ${changeset.id}`);
    }
    deferred = plan.releases.filter((r) => !scope.includes(r.name) && r.type !== "none");
    plan.releases = plan.releases.filter((r) => scope.includes(r.name));
    if (deferred.length) {
      const identity = JSON.stringify([[...ids].sort(), [...scope].sort(), deferred]);
      const suffix = createHash("sha256").update(identity).digest("hex").slice(0, 12);
      deferredChangeset = `deferred-dependencies-${suffix}`;
      deferredContent = [
        "---",
        ...[...deferred]
          .sort((a, b) => a.name.localeCompare(b.name))
          .map((r) => `${JSON.stringify(r.name)}: ${r.type}`),
        "---",
        "",
        `Retain dependency updates deferred by scoped preparation of ${[...scope].sort().join(", ")}.`,
        "Review and publish these dependent packages separately; their versions were not advanced.",
        "",
      ].join("\n");
      if (await Bun.file(join(root, `.changeset/${deferredChangeset}.md`)).exists())
        throw Error(`Deferred changeset already exists: ${deferredChangeset}`);
    }
  }
  if (options.write) {
    if (!selected.length) throw Error("No pending changesets selected");
    // Preserve downstream intent before Changesets consumes the selected files.
    if (deferredChangeset && deferredContent)
      await writeFile(join(root, `.changeset/${deferredChangeset}.md`), deferredContent, {
        flag: "wx",
      });
    const context = dirname(fileURLToPath(import.meta.resolve("@changesets/cli/package.json")));
    await applyReleasePlan(plan, packages, config, undefined, context);
  }
  return { ...plan, deferred, ...(deferredChangeset ? { deferredChangeset } : {}) };
}

if (import.meta.main) {
  try {
    const [command, ...args] = process.argv.slice(2);
    if (command !== "status" && command !== "version") throw Error("Expected status or version");
    const changesets: string[] = [];
    const packages: string[] = [];
    let output: string | undefined;
    for (let i = 0; i < args.length; i += 2) {
      const value = args[i + 1];
      if (!value || value.startsWith("--")) throw Error("Expected option value");
      if (args[i] === "--changeset") changesets.push(value);
      else if (args[i] === "--package") packages.push(value);
      else if (args[i] === "--output" && command === "status" && !output) output = value;
      else throw Error(`Unsupported option: ${args[i]}`);
    }
    const plan = await prepareVersions(resolve(import.meta.dir, ".."), {
      ...(changesets.length ? { changesets } : {}),
      ...(packages.length ? { packages } : {}),
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
