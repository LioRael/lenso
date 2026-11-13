import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const registry = process.argv[2];
if (!["npm", "crates"].includes(registry)) throw new Error("Usage: node scripts/publish.mjs npm|crates");

function run(command, args) {
  const result = spawnSync(command, args, { stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed (${result.status})`);
}

const packages = registry === "npm"
  ? ["app", "sqlite", "console"].map((directory) => {
      const path = `typescript/packages/${directory}`;
      return { ...JSON.parse(readFileSync(`${path}/package.json`, "utf8")), path };
    })
  : JSON.parse(spawnSync("cargo", ["metadata", "--manifest-path", "rust/Cargo.toml", "--no-deps", "--format-version", "1"], { encoding: "utf8" }).stdout)
      .packages.filter((pkg) => ["lenso-app", "lenso-sqlite"].includes(pkg.name))
      .sort((a, b) => a.name.localeCompare(b.name));

for (const pkg of packages) {
  const url = registry === "npm"
    ? `https://registry.npmjs.org/${encodeURIComponent(pkg.name)}/${pkg.version}`
    : `https://crates.io/api/v1/crates/${pkg.name}/${pkg.version}`;
  const response = await fetch(url, {
    headers: { "User-Agent": "Lenso-release (https://github.com/LioRael/lenso)" },
    signal: AbortSignal.timeout(30_000),
  });
  if (response.ok) {
    console.log(`${pkg.name}@${pkg.version} is already published; skipping.`);
    continue;
  }
  if (response.status !== 404) throw new Error(`Registry preflight failed: ${response.status} ${url}`);
  if (registry === "npm") run("npm", ["publish", `./${pkg.path}`, "--access", "public"]);
  else run("cargo", ["publish", "--manifest-path", "rust/Cargo.toml", "--package", pkg.name, "--locked"]);
}
