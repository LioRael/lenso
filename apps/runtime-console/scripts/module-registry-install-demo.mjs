#!/usr/bin/env node
import { createHash, generateKeyPairSync, sign } from "node:crypto";
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

const captureConsoleOutput = async (action) => {
  const originalLog = console.log;
  const logs = [];
  console.log = (...args) => {
    logs.push(args.join(" "));
  };
  try {
    await action();
  } finally {
    console.log = originalLog;
  }
  return logs.join("\n");
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
    const packageArtifact = path.join(
      modulePackagesRoot,
      "lenso-billing/billing-package.tgz"
    );
    const packageBytes = "lenso fixture billing package\n";
    const signingKeyPair = generateKeyPairSync("ed25519");
    const signatureBytes = sign(
      null,
      Buffer.from(packageBytes),
      signingKeyPair.privateKey
    );
    const publicKey = signingKeyPair.publicKey.export({
      format: "pem",
      type: "spki",
    });
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
      modulePackagesRoot,
      "lenso-billing/billing-package.tgz",
      packageBytes
    );
    await writeFixture(
      modulePackagesRoot,
      "lenso-billing/billing-package.tgz.sig",
      signatureBytes
    );
    await writeFixture(
      hostRoot,
      ".lenso/module-publishers.json",
      JSON.stringify(
        {
          publishers: [
            {
              publicKey,
              publicKeyId: "lenso-fixtures-ed25519",
              publisher: "Lenso Fixtures",
              status: "trusted",
            },
          ],
          version: 1,
        },
        null,
        2
      )
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
              installPolicy: "trusted",
              manifestReference,
              name: "billing",
              provenance: {
                checksum: `sha256:${createHash("sha256").update(packageBytes).digest("hex")}`,
                packageUrl: packageArtifact,
                publicKeyId: "lenso-fixtures-ed25519",
                publisher: "Lenso Fixtures",
                signatureAlgorithm: "ed25519-detached",
                signatureUrl: `${packageArtifact}.sig`,
                sourceRepository: "https://example.com/lenso/billing-module",
              },
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
      "doctor",
      "--repo-root",
      hostRoot,
      "--registry-file",
      registryFile,
    ]);
    const registryDoctorSnapshot = JSON.parse(
      await captureConsoleOutput(async () => {
        await runConsolePackageCli([
          "module",
          "registry",
          "doctor",
          "--repo-root",
          hostRoot,
          "--registry-file",
          registryFile,
          "--json",
        ]);
      })
    );
    if (registryDoctorSnapshot.status !== "passed") {
      throw new Error("registry doctor JSON snapshot did not pass");
    }
    if (registryDoctorSnapshot.modules[0]?.status !== "ready") {
      throw new Error("registry doctor JSON module status was not ready");
    }
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
