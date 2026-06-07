#!/usr/bin/env node
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
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
    path.join(os.tmpdir(), "lenso-module-registry-install-demo-")
  );
  try {
    const hostRoot = path.join(demoRoot, "host");
    const modulePackagesRoot = path.join(demoRoot, "module-packages");
    const registryFile = path.join(hostRoot, ".lenso/module-registry.json");
    const baseUrl = "http://127.0.0.1:4200/lenso/module/v1";
    await createHostFixture(hostRoot);

    await runConsolePackageCli([
      "module",
      "create",
      "billing",
      "--remote",
      "--output-dir",
      modulePackagesRoot,
    ]);
    const manifestReference = path.join(
      modulePackagesRoot,
      "lenso-billing/lenso.module.json"
    );
    await writeFixture(
      hostRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl,
              capabilities: ["billing.read"],
              consolePackages: [
                {
                  exportName: "billingConsoleModule",
                  packageName: "@vendor/lenso-billing-console",
                  route: "/data/billing",
                },
              ],
              manifestReference,
              name: "billing",
              source: "remote",
              summary: "Billing workspace and operations",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    await runConsolePackageCli([
      "module",
      "registry",
      "list",
      "--repo-root",
      hostRoot,
      "--registry-file",
      registryFile,
    ]);
    await runConsolePackageCli([
      "module",
      "registry",
      "inspect",
      "billing",
      "--repo-root",
      hostRoot,
      "--registry-file",
      registryFile,
    ]);
    await runConsolePackageCli([
      "module",
      "registry",
      "install",
      "billing",
      "--repo-root",
      hostRoot,
      "--registry-file",
      registryFile,
    ]);
    await runConsolePackageCli([
      "console-package",
      "apply-plan",
      "--repo-root",
      hostRoot,
    ]);
    await runConsolePackageCli(["module", "doctor", "--repo-root", hostRoot]);

    const envFile = await readFile(path.join(hostRoot, ".env"), "utf-8");
    assertContains(envFile, `REMOTE_MODULES=billing=${baseUrl}`, ".env");
    const installPlan = await readFile(
      path.join(hostRoot, ".lenso/console-package-install-plan.json"),
      "utf-8"
    );
    assertContains(installPlan, '"moduleName": "billing"', "install plan");
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

    console.log("Module registry install demo passed");
    if (process.env.LENSO_KEEP_MODULE_REGISTRY_INSTALL_DEMO) {
      console.log(`Demo root: ${demoRoot}`);
    }
  } finally {
    if (!process.env.LENSO_KEEP_MODULE_REGISTRY_INSTALL_DEMO) {
      await rm(demoRoot, { force: true, recursive: true });
    }
  }
};

await main();
