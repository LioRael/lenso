#!/usr/bin/env node
import { mkdtemp, readFile, rm, writeFile, mkdir } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { runConsolePackageCli } from "@lenso/console-package-cli";

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

const main = async () => {
  const demoRoot = await mkdtemp(
    path.join(os.tmpdir(), "lenso-remote-module-install-demo-")
  );
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
    await runConsolePackageCli([
      "module",
      "add",
      path.join(modulePackagesRoot, "lenso-billing/lenso.module.json"),
      "--repo-root",
      hostRoot,
      "--base-url",
      "http://127.0.0.1:4200/lenso/module/v1",
    ]);
    await runConsolePackageCli([
      "console-package",
      "apply-plan",
      "--repo-root",
      hostRoot,
    ]);

    const envFile = await readFile(path.join(hostRoot, ".env"), "utf-8");
    assertContains(
      envFile,
      "REMOTE_MODULES=billing=http://127.0.0.1:4200/lenso/module/v1",
      ".env"
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
    const manifestExports = await readFile(
      path.join(
        hostRoot,
        "apps/runtime-console/src/console-package-manifest-exports.ts"
      ),
      "utf-8"
    );
    assertContains(
      manifestExports,
      'billingConsoleManifest } from "@vendor/lenso-billing-console"',
      "manifest exports"
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

    console.log("Remote module install demo passed");
    if (process.env.LENSO_KEEP_REMOTE_MODULE_INSTALL_DEMO) {
      console.log(`Demo root: ${demoRoot}`);
    }
  } finally {
    if (!process.env.LENSO_KEEP_REMOTE_MODULE_INSTALL_DEMO) {
      await rm(demoRoot, { force: true, recursive: true });
    }
  }
};

await main();
