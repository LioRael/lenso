#!/usr/bin/env node
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { runConsolePackageCli } from "@lenso/console-package-cli";

import { serveHelloActionModule } from "../../../examples/remote-modules/hello-action/src/module.mjs";

const execFileAsync = promisify(execFile);
const repoRoot = path.resolve(import.meta.dirname, "../../..");

const writeFixture = async (root, relativePath, contents) => {
  const filePath = path.join(root, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, contents);
};

const createHostFixture = async (hostRoot) => {
  await writeFixture(
    hostRoot,
    "apps/runtime-console/package.json",
    JSON.stringify(
      {
        dependencies: {
          "@lenso/runtime-console-api": "workspace:*",
        },
        scripts: {
          test: "vitest run src packages/console-package-api/src",
        },
      },
      null,
      2
    )
  );
  await writeFixture(hostRoot, ".env", "APP_ENV=local\n");
  await writeFixture(
    hostRoot,
    "apps/runtime-console/src/console-package-manifest-exports.ts",
    "export const consolePackageManifests = [] as const;\n"
  );
  await writeFixture(
    hostRoot,
    "apps/runtime-console/src/console-package-module-exports.ts",
    `import type { ConsolePackageModuleExportsByKey } from "./app/console-package-registry";

export const consolePackageModuleExportsByKey = {} satisfies ConsolePackageModuleExportsByKey;
`
  );
};

const assertContains = (value, expected, label) => {
  if (!value.includes(expected)) {
    throw new Error(`${label} did not include ${expected}`);
  }
};

const main = async () => {
  const demoRoot = await mkdtemp(path.join(os.tmpdir(), "lenso-release-demo-"));
  const server = await serveHelloActionModule({ port: 0 });

  try {
    await execFileAsync(process.execPath, [
      path.join(repoRoot, "examples/remote-modules/hello-action/src/smoke.mjs"),
    ]);

    const hostRoot = path.join(demoRoot, "host");
    await createHostFixture(hostRoot);

    await runConsolePackageCli([
      "module",
      "add",
      server.manifestUrl,
      "--repo-root",
      hostRoot,
    ]);
    await runConsolePackageCli([
      "console-package",
      "apply-plan",
      "--repo-root",
      hostRoot,
    ]);

    const moduleBaseUrl = server.manifestUrl.slice(0, -"/manifest".length);
    const envFile = await readFile(path.join(hostRoot, ".env"), "utf-8");
    assertContains(
      envFile,
      `REMOTE_MODULES=hello-action=${moduleBaseUrl}`,
      ".env"
    );

    const installPlan = await readFile(
      path.join(hostRoot, ".lenso/console-package-install-plan.json"),
      "utf-8"
    );
    assertContains(installPlan, '"moduleName": "hello-action"', "install plan");

    console.log("Release demo passed");
    console.log(`Manifest URL: ${server.manifestUrl}`);
    console.log(`Install command: lenso module add ${server.manifestUrl}`);
  } finally {
    await server.close();
    if (!process.env.LENSO_KEEP_RELEASE_DEMO) {
      await rm(demoRoot, { force: true, recursive: true });
    }
  }
};

await main();
