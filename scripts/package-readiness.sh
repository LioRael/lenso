#!/usr/bin/env sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
sdk_dir="$root_dir/packages/ts-sdk"
pack_json="$(mktemp)"
metadata_json="$(mktemp)"

cleanup() {
    rm -f "$pack_json" "$metadata_json"
}
trap cleanup EXIT

echo "Checking @lenso/ts-sdk publish metadata..."
node - "$sdk_dir/package.json" <<'NODE'
const fs = require("node:fs");

const manifestPath = process.argv[2];
const pkg = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const failures = [];

if (pkg.private === true) {
  failures.push("package must not be private");
}
if (pkg.license !== "MIT") {
  failures.push("license must be MIT");
}
if (pkg.publishConfig?.access !== "public") {
  failures.push("publishConfig.access must be public");
}
if (pkg.publishConfig?.registry !== "https://registry.npmjs.org/") {
  failures.push("publishConfig.registry must target npmjs.org");
}
if (!pkg.files?.includes("dist")) {
  failures.push("files must include dist");
}
if (!pkg.exports?.["."]?.default?.startsWith("./dist/")) {
  failures.push("exports.default must point at dist output");
}
if (!pkg.exports?.["."]?.types?.startsWith("./dist/")) {
  failures.push("exports.types must point at dist declarations");
}

if (failures.length > 0) {
  console.error(failures.map((failure) => `- ${failure}`).join("\n"));
  process.exit(1);
}
NODE

echo "Building @lenso/ts-sdk..."
pnpm --dir="$sdk_dir" run build

echo "Dry-running npm pack for @lenso/ts-sdk..."
(
    cd "$sdk_dir"
    npm pack --dry-run --json > "$pack_json"
)

node - "$pack_json" <<'NODE'
const fs = require("node:fs");

const packPath = process.argv[2];
const [pack] = JSON.parse(fs.readFileSync(packPath, "utf8"));
const files = pack.files.map((entry) => entry.path).sort();
const required = [
  "LICENSE",
  "README.md",
  "dist/generated/client.d.ts",
  "dist/generated/client.js",
  "dist/generated/types.d.ts",
  "dist/generated/types.js",
  "dist/index.d.ts",
  "dist/index.js",
  "package.json",
];
const forbidden = files.filter(
  (path) =>
    path.startsWith("src/") ||
    path.startsWith("tests/") ||
    path.startsWith("node_modules/") ||
    path === "tsconfig.json" ||
    path === "tsconfig.build.json" ||
    path.endsWith(".tsbuildinfo")
);
const missing = required.filter((path) => !files.includes(path));

if (missing.length > 0 || forbidden.length > 0) {
  if (missing.length > 0) {
    console.error("Missing expected package files:");
    console.error(missing.map((path) => `- ${path}`).join("\n"));
  }
  if (forbidden.length > 0) {
    console.error("Unexpected package files:");
    console.error(forbidden.map((path) => `- ${path}`).join("\n"));
  }
  process.exit(1);
}

console.log(
  `@lenso/ts-sdk pack dry-run: ${files.length} files, ${pack.unpackedSize} unpacked bytes`
);
NODE

echo "Checking Rust workspace crate publish stance..."
cargo metadata --format-version=1 --no-deps --locked > "$metadata_json"
node - "$metadata_json" <<'NODE'
const fs = require("node:fs");

const metadataPath = process.argv[2];
const metadata = JSON.parse(fs.readFileSync(metadataPath, "utf8"));
const allowedPublishable = new Set(["lenso"]);
const publishable = metadata.packages
  .filter((pkg) => pkg.publish === null || pkg.publish === undefined || pkg.publish.length > 0)
  .map((pkg) => pkg.name);
const unexpected = publishable.filter((name) => !allowedPublishable.has(name));
const missing = [...allowedPublishable].filter((name) => !publishable.includes(name));

if (unexpected.length > 0 || missing.length > 0) {
  if (unexpected.length > 0) {
    console.error("Only the public facade crate may be publishable; keep these crates internal:");
    console.error(unexpected.map((name) => `- ${name}`).join("\n"));
  }
  if (missing.length > 0) {
    console.error("Expected public facade crates are not publishable:");
    console.error(missing.map((name) => `- ${name}`).join("\n"));
  }
  process.exit(1);
}

console.log(
  `Rust workspace stance: ${publishable.join(", ")} is publishable; other workspace crates remain internal.`
);
NODE

echo "Dry-running cargo package for lenso facade..."
cargo package --locked -p lenso --allow-dirty

echo "Package readiness checks passed."
