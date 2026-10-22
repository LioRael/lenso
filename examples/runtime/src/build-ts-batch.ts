import { generate } from "@lenso/engine";
import { resolve } from "node:path";

const appRoot = resolve(import.meta.dir, "..");
await generate(appRoot);
const result = await Bun.build({
  entrypoints: [resolve(appRoot, "src/ts-batch.ts")],
  target: "bun",
  packages: "external",
});
if (!result.success) throw new AggregateError(result.logs, "TS candidate build failed.");
await Bun.write(resolve(appRoot, ".lenso/ts-batch/server.js"), result.outputs[0]!);
