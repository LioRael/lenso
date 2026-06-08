#!/usr/bin/env node
import { execFile, spawn } from "node:child_process";
import { once } from "node:events";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { runConsolePackageCli } from "@lenso/console-package-cli";

const execFileAsync = promisify(execFile);
const writeFixture = async (root, relativePath, contents) => {
  const filePath = path.join(root, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, contents);
};

const createHostFixture = async (repoRoot) => {
  await writeFixture(
    repoRoot,
    "Cargo.toml",
    `[workspace]
resolver = "2"
members = [
    "crates/app-bootstrap",
]
`
  );
  await writeFixture(
    repoRoot,
    "crates/app-bootstrap/src/lib.rs",
    `const LINKED_MODULE_ENTRIES: &[LinkedModuleEntry] = &[
];
`
  );
  await writeFixture(
    repoRoot,
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
  await writeFixture(
    repoRoot,
    "apps/runtime-console/src/console-package-manifest-exports.ts",
    `export const consolePackageManifests = [
] as const;
`
  );
  await writeFixture(
    repoRoot,
    "apps/runtime-console/src/console-package-module-exports.ts",
    `import {
  consolePackageKey,
  type ConsolePackageModuleExportsByKey,
} from "./app/console-package-registry";

export const consolePackageModuleExportsByKey = {
} satisfies ConsolePackageModuleExportsByKey;
`
  );
};

const assertContains = (value, expected, label) => {
  if (!value.includes(expected)) {
    throw new Error(`${label} did not include ${expected}`);
  }
};

const readManifestUrlFromProcess = async (childProcess) => {
  const timeout = setTimeout(() => childProcess.kill(), 3000);
  try {
    for await (const chunk of childProcess.stdout) {
      const manifestUrl = String(chunk).match(/http:\/\/\S+/u)?.[0];
      if (manifestUrl) {
        return manifestUrl;
      }
    }
  } finally {
    clearTimeout(timeout);
  }
  throw new Error("remote module backend did not print manifest URL");
};

const stopProcess = async (childProcess) => {
  if (!childProcess || childProcess.killed) {
    return;
  }
  childProcess.kill();
  await once(childProcess, "exit").catch(() => {
    /* process may already be gone */
  });
};

const main = async () => {
  const demoRoot = await mkdtemp(
    path.join(os.tmpdir(), "lenso-remote-module-package-demo-")
  );
  let backendProcess = null;
  try {
    const hostRoot = path.join(demoRoot, "host");
    const modulePackagesRoot = path.join(demoRoot, "module-packages");
    await createHostFixture(hostRoot);

    await runConsolePackageCli([
      "module",
      "create",
      "billing",
      "--remote",
      "--output-dir",
      modulePackagesRoot,
    ]);

    const packageRoot = path.join(modulePackagesRoot, "lenso-billing");
    const rootPackageJson = await readFile(
      path.join(packageRoot, "package.json"),
      "utf-8"
    );
    assertContains(
      rootPackageJson,
      '"smoke": "pnpm --dir backend smoke"',
      "package scripts"
    );
    await execFileAsync("pnpm", ["--dir", packageRoot, "smoke"]);

    const catalogEntry = JSON.parse(
      await readFile(path.join(packageRoot, "catalog-entry.json"), "utf-8")
    );
    if (
      catalogEntry.name !== "billing" ||
      catalogEntry.version !== "0.1.0" ||
      catalogEntry.consolePackages?.[0]?.packageName !==
        "@vendor/lenso-billing-console"
    ) {
      throw new Error("catalog-entry.json did not match generated module");
    }

    backendProcess = spawn(process.execPath, ["src/server.mjs"], {
      cwd: path.join(packageRoot, "backend"),
      env: { ...process.env, PORT: "0" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const manifestUrl = await readManifestUrlFromProcess(backendProcess);

    await runConsolePackageCli([
      "module",
      "catalog",
      "add",
      manifestUrl,
      "--repo-root",
      hostRoot,
      "--summary",
      catalogEntry.summary,
    ]);
    await runConsolePackageCli([
      "module",
      "add",
      manifestUrl,
      "--repo-root",
      hostRoot,
    ]);
    await runConsolePackageCli([
      "console-package",
      "apply-plan",
      "--repo-root",
      hostRoot,
    ]);

    const catalogFile = await readFile(
      path.join(hostRoot, ".lenso/module-catalog.json"),
      "utf-8"
    );
    assertContains(
      catalogFile,
      `"manifestReference": "${manifestUrl}"`,
      "catalog"
    );
    assertContains(
      catalogFile,
      '"summary": "Billing workspace and operations"',
      "catalog"
    );
    const envFile = await readFile(path.join(hostRoot, ".env"), "utf-8");
    assertContains(
      envFile,
      `REMOTE_MODULES=billing=${manifestUrl.slice(0, -"/manifest".length)}`,
      ".env"
    );
    const installPlan = await readFile(
      path.join(hostRoot, ".lenso/console-package-install-plan.json"),
      "utf-8"
    );
    assertContains(
      installPlan,
      '"packageName": "@vendor/lenso-billing-console"',
      "install plan"
    );
    const packageJson = await readFile(
      path.join(hostRoot, "apps/runtime-console/package.json"),
      "utf-8"
    );
    assertContains(
      packageJson,
      '"@vendor/lenso-billing-console": "latest"',
      "Runtime Console package.json"
    );
    const moduleExports = await readFile(
      path.join(
        hostRoot,
        "apps/runtime-console/src/console-package-module-exports.ts"
      ),
      "utf-8"
    );
    assertContains(
      moduleExports,
      "[consolePackageKey(billingConsoleManifest)]: billingConsoleModule",
      "module exports"
    );

    console.log("Remote module package demo passed");
    if (process.env.LENSO_KEEP_REMOTE_MODULE_INSTALL_DEMO) {
      console.log(`Demo root: ${demoRoot}`);
    }
  } finally {
    await stopProcess(backendProcess);
    if (!process.env.LENSO_KEEP_REMOTE_MODULE_INSTALL_DEMO) {
      await rm(demoRoot, { force: true, recursive: true });
    }
  }
};

await main();
