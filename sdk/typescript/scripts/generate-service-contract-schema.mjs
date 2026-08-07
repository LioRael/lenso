import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const workspaceRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const sourcePath = path.join(
  workspaceRoot,
  "..",
  "..",
  "crates",
  "lenso-service",
  "schemas",
  "lenso-service.v1.schema.json",
);
const outputPath = path.join(
  workspaceRoot,
  "packages",
  "service-kit",
  "src",
  "generated",
  "service-contract-schema.ts",
);

const source = readFileSync(sourcePath, "utf8");
const schema = JSON.parse(source);
const generated = `// Generated from crates/lenso-service/schemas/lenso-service.v1.schema.json. Do not edit.\nexport const serviceContractSchema = ${JSON.stringify(schema, null, 2)} as const;\n`;

if (process.argv.includes("--check")) {
  if (!existsSync(outputPath)) {
    console.error(`Missing generated Service Contract schema: ${outputPath}`);
    process.exit(1);
  }
  const current = readFileSync(outputPath, "utf8");
  if (current !== generated) {
    console.error(
      "Generated Service Contract schema is stale; run pnpm generate:service-contract-schema",
    );
    process.exit(1);
  }
  console.log("generated Service Contract schema is up to date");
} else {
  mkdirSync(path.dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, generated);
  console.log(`generated ${path.relative(workspaceRoot, outputPath)}`);
}
