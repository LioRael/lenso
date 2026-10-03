import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
const root = path.dirname(fileURLToPath(import.meta.url));
if (!process.argv[2]) throw new Error("Pass the exact lenso-js source checkout directory");
const sdk = fs.realpathSync(path.join(process.argv[2], "packages/lenso-bun-plugin"));
const manifest = JSON.parse(fs.readFileSync(path.join(sdk, "package.json"), "utf8"));
if (!manifest.exports?.["./authoring"] || !manifest.exports?.["./targets"])
  throw new Error("This source checkout must expose authoring and target packaging");
const file = path.join(root, "package.json");
const app = JSON.parse(fs.readFileSync(file, "utf8"));
app.dependencies["@lenso/bun-plugin"] = "file:" + path.relative(root, sdk);
fs.writeFileSync(file, JSON.stringify(app, null, 2) + "\n");
const result = spawnSync("bun", ["install"], { cwd: root, stdio: "inherit" });
process.exit(result.status ?? 1);
