import { execFile } from "node:child_process";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { once } from "node:events";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { promisify } from "node:util";

import { afterEach, describe, expect, test, vi } from "vitest";

import { runConsolePackageCli } from "./index.mjs";

const tempRoots = [];
const tempServers = [];
const execFileAsync = promisify(execFile);
const registryPackageBytes = Buffer.from("lenso fixture billing package\n");
const registrySigningKeyPair = generateKeyPairSync("ed25519");
const registrySignatureBytes = sign(
  null,
  registryPackageBytes,
  registrySigningKeyPair.privateKey
);
const registryPublicKeyPem = registrySigningKeyPair.publicKey
  .export({
    format: "pem",
    type: "spki",
  })
  .trim();
const registryProvenance = {
  checksum: `sha256:${createHash("sha256").update(registryPackageBytes).digest("hex")}`,
  packageUrl: "https://example.com/lenso/module/v1/package.tgz",
  publicKeyId: "lenso-fixtures-ed25519",
  publisher: "Lenso Fixtures",
  signatureAlgorithm: "ed25519-detached",
  signatureUrl: "https://example.com/lenso/module/v1/package.tgz.sig",
  sourceRepository: "https://example.com/lenso/billing-module",
};

const registryProvenanceForManifestUrl = (manifestUrl) => ({
  ...registryProvenance,
  packageUrl: manifestUrl.replace(/\/manifest$/u, "/package.tgz"),
  signatureUrl: manifestUrl.replace(/\/manifest$/u, "/package.tgz.sig"),
});

const createRepoFixture = async () => {
  const repoRoot = await mkdtemp(path.join(os.tmpdir(), "lenso-module-cli-"));
  tempRoots.push(repoRoot);
  await writeFixture(
    repoRoot,
    "crates/app-bootstrap/Cargo.toml",
    `[package]
name = "app-bootstrap"
version = "0.1.0"
edition.workspace = true

[dependencies]
platform-core.workspace = true
platform-module.workspace = true
`
  );
  await writeFixture(
    repoRoot,
    "Cargo.toml",
    `[workspace]
resolver = "2"
members = [
    "crates/app-bootstrap",
]

[workspace.package]
edition = "2024"
license = "UNLICENSED"
publish = false
rust-version = "1.94"

[workspace.dependencies]
platform-core = { path = "crates/platform-core" }
platform-module = { path = "crates/platform-module" }
app-bootstrap = { path = "crates/app-bootstrap" }
`
  );
  await writeFixture(
    repoRoot,
    ".lenso/module-publishers.json",
    JSON.stringify(
      {
        publishers: [
          {
            notes: "Fixture publisher key",
            publicKey: registryPublicKeyPem,
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
    repoRoot,
    "crates/app-bootstrap/src/lib.rs",
    `const LINKED_MODULE_ENTRIES: &[LinkedModuleEntry] = &[
    LinkedModuleEntry {
        module_name: "platform-story",
        manifest: platform_story_manifest,
        load: platform_story_module,
        http_binding: None,
    },
];
`
  );
  return repoRoot;
};

const createRuntimeConsoleFixture = async (repoRoot) => {
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
    "apps/runtime-console/tsconfig.json",
    JSON.stringify(
      {
        compilerOptions: {
          paths: {
            "@lenso/runtime-console-api": [
              "./packages/console-package-api/src/index.ts",
            ],
          },
        },
        include: ["src", "packages/console-package-api/src"],
      },
      null,
      2
    )
  );
  await writeFixture(
    repoRoot,
    "apps/runtime-console/vite.config.ts",
    `export default {
  resolve: {
    alias: {
      "@lenso/runtime-console-api": fileURLToPath(
        new URL("packages/console-package-api/src/index.ts", import.meta.url)
      ),
    },
  },
};
`
  );
  await writeFixture(
    repoRoot,
    "apps/runtime-console/oxlint.config.ts",
    `export default {
  overrides: [
    {
      files: [
        "vite.config.ts",
      ],
    },
  ],
};
`
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

const writeFixture = async (repoRoot, relativePath, contents) => {
  const { mkdir, writeFile } = await import("node:fs/promises");
  const filePath = path.join(repoRoot, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, contents);
};

const writeRegistryInstallHistoryFixture = async (repoRoot) => {
  await writeFixture(
    repoRoot,
    ".lenso/module-registry-install-history.json",
    JSON.stringify(
      {
        entries: [
          {
            action: "registry.install",
            baseUrl: "https://example.com/lenso/module/v1",
            catalogVersion: "0.1.0",
            consolePackageHints: 1,
            installPolicy: "trusted",
            installedAt: "2026-06-07T12:00:00.000Z",
            manifestReference: "https://example.com/lenso/module/v1/manifest",
            moduleName: "billing",
            provenance: registryProvenance,
            source: "remote",
          },
        ],
        version: 1,
      },
      null,
      2
    )
  );
};

afterEach(async () => {
  await Promise.all(
    tempServers.splice(0).map(async (server) => {
      server.close();
      await once(server, "close");
    })
  );
  await Promise.all(
    tempRoots
      .splice(0)
      .map((root) => rm(root, { force: true, recursive: true }))
  );
});

const serveManifest = async (manifest) => {
  const server = createServer((request, response) => {
    if (request.url === "/lenso/module/v1/manifest") {
      response.setHeader("Content-Type", "application/json");
      response.end(JSON.stringify(manifest));
      return;
    }
    if (request.url === "/lenso/module/v1/package.tgz") {
      response.setHeader("Content-Type", "application/octet-stream");
      response.end(registryPackageBytes);
      return;
    }
    if (request.url === "/lenso/module/v1/package.tgz.sig") {
      response.setHeader("Content-Type", "text/plain");
      response.end(registrySignatureBytes);
      return;
    }
    response.statusCode = 404;
    response.end("not found");
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  tempServers.push(server);
  const { port } = server.address();
  return `http://127.0.0.1:${port}/lenso/module/v1/manifest`;
};

const captureConsoleLogs = async (action) => {
  const logs = [];
  const logSpy = vi.spyOn(console, "log").mockImplementation((...args) => {
    logs.push(args.join(" "));
  });
  try {
    await action();
  } finally {
    logSpy.mockRestore();
  }
  return logs.join("\n");
};

describe("module scaffold CLI", () => {
  test("uses commander for command and option parsing", async () => {
    const packageJson = JSON.parse(
      await readFile(new URL("../package.json", import.meta.url), "utf-8")
    );

    expect(packageJson.dependencies).toHaveProperty("commander");
  });

  test("accepts pnpm forwarded arguments after a separator", async () => {
    const outputRoot = await mkdtemp(
      path.join(os.tmpdir(), "lenso-forwarded-cli-")
    );
    tempRoots.push(outputRoot);

    await expect(
      runConsolePackageCli([
        "module",
        "create",
        "--",
        "billing",
        "--remote",
        "--output-dir",
        outputRoot,
      ])
    ).resolves.toBe(0);

    await expect(
      readFile(
        path.join(outputRoot, "lenso-billing/lenso.module.json"),
        "utf-8"
      )
    ).resolves.toContain('"source": "remote"');
  });

  test("creates a linked Rust module and registers it in the workspace", async () => {
    const repoRoot = await createRepoFixture();

    await expect(
      runConsolePackageCli([
        "module",
        "create",
        "billing",
        "--repo-root",
        repoRoot,
      ])
    ).resolves.toBe(0);

    await expect(
      readFile(path.join(repoRoot, "modules/billing/Cargo.toml"), "utf-8")
    ).resolves.toContain('name = "billing"');
    await expect(
      readFile(path.join(repoRoot, "modules/billing/src/lib.rs"), "utf-8")
    ).resolves.toContain("pub mod module;");
    await expect(
      readFile(path.join(repoRoot, "modules/billing/src/module.rs"), "utf-8")
    ).resolves.toContain('ModuleManifest::builder("billing")');

    const cargoToml = await readFile(
      path.join(repoRoot, "Cargo.toml"),
      "utf-8"
    );
    expect(cargoToml).toContain('"modules/billing"');
    expect(cargoToml).toContain('billing = { path = "modules/billing" }');
    await expect(
      readFile(path.join(repoRoot, "crates/app-bootstrap/Cargo.toml"), "utf-8")
    ).resolves.toContain("billing.workspace = true");

    await expect(
      readFile(path.join(repoRoot, "crates/app-bootstrap/src/lib.rs"), "utf-8")
    ).resolves.toContain('module_name: "billing"');
  });

  test("finds the repo root when invoked from the runtime console app", async () => {
    const repoRoot = await createRepoFixture();
    const runtimeConsoleRoot = path.join(repoRoot, "apps/runtime-console");
    await writeFixture(runtimeConsoleRoot, "package.json", "{}\n");
    const previousCwd = process.cwd();
    process.chdir(runtimeConsoleRoot);
    try {
      await expect(
        runConsolePackageCli(["module", "create", "analytics"])
      ).resolves.toBe(0);
    } finally {
      process.chdir(previousCwd);
    }

    await expect(
      readFile(path.join(repoRoot, "modules/analytics/Cargo.toml"), "utf-8")
    ).resolves.toContain('name = "analytics"');
  });

  test("creates a linked module with a registered console package", async () => {
    const repoRoot = await createRepoFixture();
    await createRuntimeConsoleFixture(repoRoot);
    const logs = [];
    const logSpy = vi.spyOn(console, "log").mockImplementation((...args) => {
      logs.push(args.join(" "));
    });

    try {
      await expect(
        runConsolePackageCli([
          "module",
          "create",
          "billing",
          "--repo-root",
          repoRoot,
          "--with-console",
        ])
      ).resolves.toBe(0);
    } finally {
      logSpy.mockRestore();
    }
    expect(logs.join("\n")).toContain(
      'Keep navigation.workspace.id="billing" so the module owns its workspace'
    );

    const moduleSource = await readFile(
      path.join(repoRoot, "modules/billing/src/module.rs"),
      "utf-8"
    );
    expect(moduleSource).toContain(
      "use platform_module::{ConsoleArea, ConsolePackage, ConsoleSurface, LinkedBinding, Module, ModuleManifest};"
    );
    expect(moduleSource).toContain(
      '.capabilities(vec!["billing.read".to_owned()])'
    );
    expect(moduleSource).toContain(".console(vec![ConsoleSurface {");
    expect(moduleSource).toContain('name: "@lenso/billing-console".to_owned()');
    expect(moduleSource).toContain('export: "billingConsoleModule".to_owned()');
    const consoleSurface = JSON.parse(
      await readFile(
        path.join(
          repoRoot,
          "apps/runtime-console/packages/billing-console/console-surface.json"
        ),
        "utf-8"
      )
    );
    expect(consoleSurface).toMatchObject({
      navigation: {
        order: 10,
        workspace: {
          icon: "database",
          id: "billing",
          label: "Billing",
        },
      },
    });

    const packageSource = await readFile(
      path.join(
        repoRoot,
        "apps/runtime-console/packages/billing-console/src/index.tsx"
      ),
      "utf-8"
    );
    expect(packageSource).toContain("billingConsoleModule");
    expect(packageSource).toContain(
      "navigation: billingConsoleManifest.navigation"
    );
    await expect(
      readFile(
        path.join(repoRoot, "apps/runtime-console/package.json"),
        "utf-8"
      )
    ).resolves.toContain('"@lenso/billing-console": "workspace:*"');
  });

  test("applies a console package install plan to runtime console registration", async () => {
    const repoRoot = await createRepoFixture();
    await createRuntimeConsoleFixture(repoRoot);
    await writeFixture(
      repoRoot,
      ".lenso/console-package-install-plan.json",
      JSON.stringify(
        {
          modules: [
            {
              consolePackages: [
                {
                  exportName: "billingConsoleModule",
                  packageName: "@vendor/lenso-billing-console",
                  requestedByModule: "billing",
                  route: "/data/billing",
                },
              ],
              moduleName: "billing",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    for (let index = 0; index < 2; index += 1) {
      await expect(
        runConsolePackageCli([
          "console-package",
          "apply-plan",
          "--repo-root",
          repoRoot,
        ])
      ).resolves.toBe(0);
    }

    const packageJson = JSON.parse(
      await readFile(
        path.join(repoRoot, "apps/runtime-console/package.json"),
        "utf-8"
      )
    );
    expect(packageJson.dependencies).toMatchObject({
      "@vendor/lenso-billing-console": "latest",
    });

    const manifestExports = await readFile(
      path.join(
        repoRoot,
        "apps/runtime-console/src/console-package-manifest-exports.ts"
      ),
      "utf-8"
    );
    expect(manifestExports).toContain(
      'import { billingConsoleManifest } from "@vendor/lenso-billing-console";'
    );
    expect(manifestExports.match(/billingConsoleManifest/gu)).toHaveLength(2);

    const moduleExports = await readFile(
      path.join(
        repoRoot,
        "apps/runtime-console/src/console-package-module-exports.ts"
      ),
      "utf-8"
    );
    expect(moduleExports).toContain(
      'import { billingConsoleManifest, billingConsoleModule } from "@vendor/lenso-billing-console";'
    );
    expect(moduleExports).toContain(
      "[consolePackageKey(billingConsoleManifest)]: billingConsoleModule"
    );
    expect(moduleExports.match(/billingConsoleModule/gu)).toHaveLength(2);
  });

  test("passes module doctor after remote module frontend registration", async () => {
    const repoRoot = await createRepoFixture();
    await createRuntimeConsoleFixture(repoRoot);
    await writeFixture(
      repoRoot,
      ".env",
      "REMOTE_MODULES=billing=http://127.0.0.1:4200/lenso/module/v1\n"
    );
    await writeFixture(
      repoRoot,
      ".lenso/console-package-install-plan.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: "http://127.0.0.1:4200/lenso/module/v1",
              consolePackages: [
                {
                  exportName: "billingConsoleModule",
                  packageName: "@vendor/lenso-billing-console",
                  requestedByModule: "billing",
                  route: "/data/billing",
                },
              ],
              moduleName: "billing",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );
    await runConsolePackageCli([
      "console-package",
      "apply-plan",
      "--repo-root",
      repoRoot,
    ]);

    await expect(
      runConsolePackageCli(["module", "doctor", "--repo-root", repoRoot])
    ).resolves.toBe(0);
  });

  test("lists remote modules from a registry catalog", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              capabilities: ["billing.read"],
              consolePackages: [
                {
                  exportName: "billingConsoleModule",
                  packageName: "@vendor/lenso-billing-console",
                  route: "/data/billing",
                },
              ],
              installPolicy: "trusted",
              manifestReference: "https://example.com/lenso/module/v1/manifest",
              name: "billing",
              provenance: registryProvenance,
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

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "list",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Module registry entries:");
    expect(logs).toContain("billing 0.1.0 remote");
    expect(logs).toContain("Billing workspace and operations");
    expect(logs).toContain(
      "manifest: https://example.com/lenso/module/v1/manifest"
    );
    expect(logs).toContain("install policy: trusted");
    expect(logs).toContain("capabilities: billing.read");
    expect(logs).toContain(
      "console packages: @vendor/lenso-billing-console#billingConsoleModule"
    );
  });

  test("adds a remote module to a registry catalog", async () => {
    const repoRoot = await createRepoFixture();

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "add",
          "billing",
          "--repo-root",
          repoRoot,
          "--manifest",
          "https://example.com/lenso/module/v1/manifest",
          "--base-url",
          "https://example.com/lenso/module/v1/",
          "--version",
          "0.1.0",
          "--summary",
          "Billing workspace and operations",
          "--trusted",
          "--capability",
          "billing.read",
          "--console-package",
          "@vendor/lenso-billing-console#billingConsoleModule",
          "--route",
          "/data/billing",
          "--publisher",
          "Lenso Fixtures",
          "--source-repository",
          "https://example.com/lenso/billing-module",
          "--package-url",
          "https://example.com/lenso/module/v1/package.tgz",
          "--checksum",
          registryProvenance.checksum,
          "--signature-url",
          "https://example.com/lenso/module/v1/package.tgz.sig",
          "--public-key-id",
          "lenso-fixtures-ed25519",
          "--min-lenso-version",
          "0.1.0",
          "--console-package-api",
          "1",
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Added registry module billing.");
    expect(logs).toContain("- catalog: .lenso/module-registry.json");
    const registry = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-registry.json"),
        "utf-8"
      )
    );
    expect(registry).toEqual({
      modules: [
        {
          baseUrl: "https://example.com/lenso/module/v1",
          capabilities: ["billing.read"],
          compatibility: {
            consolePackageApi: "1",
            lenso: {
              minVersion: "0.1.0",
            },
          },
          consolePackages: [
            {
              exportName: "billingConsoleModule",
              packageName: "@vendor/lenso-billing-console",
              route: "/data/billing",
            },
          ],
          installPolicy: "trusted",
          manifestReference: "https://example.com/lenso/module/v1/manifest",
          name: "billing",
          provenance: {
            checksum: registryProvenance.checksum,
            packageUrl: "https://example.com/lenso/module/v1/package.tgz",
            publicKeyId: "lenso-fixtures-ed25519",
            publisher: "Lenso Fixtures",
            signatureAlgorithm: "ed25519-detached",
            signatureUrl: "https://example.com/lenso/module/v1/package.tgz.sig",
            sourceRepository: "https://example.com/lenso/billing-module",
          },
          source: "remote",
          summary: "Billing workspace and operations",
          version: "0.1.0",
        },
      ],
      version: 1,
    });
  });

  test("updates an existing registry catalog entry", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              installPolicy: "review_required",
              manifestReference: "https://old.example.com/manifest",
              name: "billing",
              source: "remote",
              version: "0.0.1",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "add",
          "billing",
          "--repo-root",
          repoRoot,
          "--manifest",
          "https://example.com/lenso/module/v1/manifest",
          "--version",
          "0.1.0",
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      module: {
        installPolicy: "review_required",
        manifestReference: "https://example.com/lenso/module/v1/manifest",
        name: "billing",
        version: "0.1.0",
      },
      registryFile: path.join(repoRoot, ".lenso/module-registry.json"),
    });
    const registry = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-registry.json"),
        "utf-8"
      )
    );
    expect(registry.modules).toHaveLength(1);
    expect(registry.modules[0]).toMatchObject({
      manifestReference: "https://example.com/lenso/module/v1/manifest",
      version: "0.1.0",
    });
  });

  test("archives a registry catalog entry by default", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              installPolicy: "trusted",
              manifestReference: "https://example.com/lenso/module/v1/manifest",
              name: "billing",
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "remove",
          "billing",
          "--repo-root",
          repoRoot,
          "--reason",
          "replaced by billing-v2",
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Archived registry module billing.");
    const registry = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-registry.json"),
        "utf-8"
      )
    );
    expect(registry.modules[0]).toMatchObject({
      archiveReason: "replaced by billing-v2",
      installPolicy: "review_required",
      name: "billing",
    });
    expect(registry.modules[0].archivedAt).toEqual(expect.any(String));
    const history = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-registry-install-history.json"),
        "utf-8"
      )
    );
    expect(history.entries[0]).toMatchObject({
      action: "registry.archive",
      catalogVersion: "0.1.0",
      moduleName: "billing",
      reason: "replaced by billing-v2",
    });
  });

  test("deletes a registry catalog entry when requested", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              installPolicy: "review_required",
              manifestReference: "https://example.com/lenso/module/v1/manifest",
              name: "billing",
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "remove",
          "billing",
          "--repo-root",
          repoRoot,
          "--delete",
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      action: "deleted",
      module: {
        name: "billing",
        version: "0.1.0",
      },
    });
    const registry = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-registry.json"),
        "utf-8"
      )
    );
    expect(registry.modules).toEqual([]);
  });

  test("blocks registry review for archived catalog entries", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              archiveReason: "replaced by billing-v2",
              archivedAt: "2026-06-07T12:00:00.000Z",
              installPolicy: "review_required",
              manifestReference: "https://example.com/lenso/module/v1/manifest",
              name: "billing",
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "review",
          "billing",
          "--repo-root",
          repoRoot,
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      decision: "blocked",
      issues: [
        {
          group: "Catalog",
          message: "billing is archived",
        },
      ],
      module: {
        archiveReason: "replaced by billing-v2",
        archivedAt: "2026-06-07T12:00:00.000Z",
        manifestStatus: "archived",
      },
    });
  });

  test("restores an archived registry catalog entry", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              archiveReason: "replaced by billing-v2",
              archivedAt: "2026-06-07T12:00:00.000Z",
              installPolicy: "review_required",
              manifestReference: "https://example.com/lenso/module/v1/manifest",
              name: "billing",
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "restore",
          "billing",
          "--repo-root",
          repoRoot,
          "--reason",
          "billing-v2 rollback",
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      action: "restored",
      module: {
        installPolicy: "review_required",
        name: "billing",
        version: "0.1.0",
      },
    });
    const registry = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-registry.json"),
        "utf-8"
      )
    );
    expect(registry.modules[0]).toMatchObject({
      installPolicy: "review_required",
      name: "billing",
    });
    expect(registry.modules[0]).not.toHaveProperty("archivedAt");
    expect(registry.modules[0]).not.toHaveProperty("archiveReason");
    const history = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-registry-install-history.json"),
        "utf-8"
      )
    );
    expect(history.entries[0]).toMatchObject({
      action: "registry.restore",
      catalogVersion: "0.1.0",
      moduleName: "billing",
      reason: "billing-v2 rollback",
    });
    expect(history.entries[0].restoredAt).toEqual(expect.any(String));
  });

  test("rejects restoring active registry catalog entries", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              installPolicy: "review_required",
              manifestReference: "https://example.com/lenso/module/v1/manifest",
              name: "billing",
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    await expect(
      runConsolePackageCli([
        "module",
        "registry",
        "restore",
        "billing",
        "--repo-root",
        repoRoot,
      ])
    ).rejects.toThrow("Registry module billing is not archived");
  });

  test("lists configured module publisher keys", async () => {
    const repoRoot = await createRepoFixture();

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "publisher",
          "list",
          "--repo-root",
          repoRoot,
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Module publisher keys:");
    expect(logs).toContain("- registry: .lenso/module-publishers.json");
    expect(logs).toContain("- Lenso Fixtures lenso-fixtures-ed25519 trusted");
    expect(logs).toContain("notes: Fixture publisher key");
  });

  test("prints module publisher keys as JSON", async () => {
    const repoRoot = await createRepoFixture();

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "publisher",
          "list",
          "--repo-root",
          repoRoot,
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      count: 1,
      publishers: [
        {
          publicKeyId: "lenso-fixtures-ed25519",
          publisher: "Lenso Fixtures",
          status: "trusted",
        },
      ],
      publishersFile: path.join(repoRoot, ".lenso/module-publishers.json"),
      version: 1,
    });
  });

  test("checks module publisher keys", async () => {
    const repoRoot = await createRepoFixture();

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "publisher",
          "doctor",
          "--repo-root",
          repoRoot,
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Module publisher doctor passed.");
    expect(logs).toContain("- registry: .lenso/module-publishers.json");
    expect(logs).toContain("- publisher keys: 1");
  });

  test("prints module publisher doctor JSON for invalid keys", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      ".lenso/module-publishers.json",
      JSON.stringify(
        {
          publishers: [
            {
              publicKey: "not a pem key",
              publicKeyId: "acme-ed25519",
              publisher: "Acme Billing",
              status: "unknown",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "publisher",
          "doctor",
          "--repo-root",
          repoRoot,
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      count: 1,
      issues: [
        {
          fix: 'set publisher key "Acme Billing" "acme-ed25519" status to trusted, review_required, or revoked',
          group: "Publisher",
          message: "Acme Billing acme-ed25519 status unknown is unsupported",
        },
        {
          fix: 'replace publisher key "Acme Billing" "acme-ed25519" with a valid PEM public key',
          group: "Publisher",
        },
      ],
      publishers: [
        {
          publicKeyId: "acme-ed25519",
          publisher: "Acme Billing",
          status: "unknown",
        },
      ],
      publishersFile: path.join(repoRoot, ".lenso/module-publishers.json"),
      status: "failed",
      version: 1,
    });
  });

  test("trusts a module publisher key from a PEM file", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      ".lenso/acme-public-key.pem",
      registryPublicKeyPem
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "publisher",
          "trust",
          "Acme Billing",
          "acme-ed25519",
          "--repo-root",
          repoRoot,
          "--public-key-file",
          path.join(repoRoot, ".lenso/acme-public-key.pem"),
          "--notes",
          "Reviewed by platform ops",
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Trusted publisher key Acme Billing acme-ed25519.");
    const registry = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-publishers.json"),
        "utf-8"
      )
    );
    expect(registry).toMatchObject({
      publishers: [
        {
          notes: "Reviewed by platform ops",
          publicKey: registryPublicKeyPem,
          publicKeyId: "acme-ed25519",
          publisher: "Acme Billing",
          status: "trusted",
        },
        {
          publicKeyId: "lenso-fixtures-ed25519",
          publisher: "Lenso Fixtures",
          status: "trusted",
        },
      ],
      version: 1,
    });
  });

  test("revokes a configured module publisher key", async () => {
    const repoRoot = await createRepoFixture();

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "publisher",
          "revoke",
          "Lenso Fixtures",
          "lenso-fixtures-ed25519",
          "--repo-root",
          repoRoot,
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain(
      "Revoked publisher key Lenso Fixtures lenso-fixtures-ed25519."
    );
    const registry = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-publishers.json"),
        "utf-8"
      )
    );
    expect(registry.publishers[0]).toMatchObject({
      publicKeyId: "lenso-fixtures-ed25519",
      publisher: "Lenso Fixtures",
      status: "revoked",
    });
  });

  test("inspects a registry module against its remote manifest", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [
        {
          package: {
            export: "billingConsoleModule",
            name: "@vendor/lenso-billing-console",
          },
          route: "/data/billing",
        },
      ],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              capabilities: ["billing.read"],
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: registryProvenanceForManifestUrl(manifestUrl),
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "inspect",
          "billing",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Registry module billing");
    expect(logs).toContain("catalog version: 0.1.0");
    expect(logs).toContain("manifest version: 0.1.0");
    expect(logs).toContain("install policy: trusted");
    expect(logs).toContain("manifest status: ok");
    expect(logs).toContain("capabilities: billing.read");
    expect(logs).toContain(
      "console packages: @vendor/lenso-billing-console#billingConsoleModule"
    );
  });

  test("reviews a registry module before installation", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [
        {
          package: {
            export: "billingConsoleModule",
            name: "@vendor/lenso-billing-console",
          },
          route: "/data/billing",
        },
      ],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              capabilities: ["billing.read"],
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: registryProvenanceForManifestUrl(manifestUrl),
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "review",
          "billing",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Registry review billing");
    expect(logs).toContain("decision: ready_to_install");
    expect(logs).toContain("install policy: trusted");
    expect(logs).toContain("manifest status: ok");
    expect(logs).toContain("issues: -");
    expect(logs).toContain("next: lenso module registry install billing");
  });

  test("prints registry module review as JSON", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              capabilities: ["billing.read"],
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: registryProvenanceForManifestUrl(manifestUrl),
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "review",
          "billing",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      decision: "ready_to_install",
      issues: [],
      module: {
        baseUrl: manifestUrl.slice(0, -"/manifest".length),
        catalogVersion: "0.1.0",
        installPolicy: "trusted",
        manifestName: "billing",
        manifestStatus: "ok",
        manifestVersion: "0.1.0",
        name: "billing",
        provenance: registryProvenanceForManifestUrl(manifestUrl),
        source: "remote",
      },
      version: 1,
    });
  });

  test("blocks registry review for incompatible host versions", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              compatibility: {
                consolePackageApi: "2",
                lenso: {
                  minVersion: "0.2.0",
                },
              },
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: registryProvenanceForManifestUrl(manifestUrl),
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "review",
          "billing",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
          "--json",
        ])
      ).resolves.toBe(0);
    });

    const snapshot = JSON.parse(logs);
    expect(snapshot).toMatchObject({
      decision: "blocked",
      issues: [
        {
          group: "Compatibility",
          message: "billing requires Lenso >= 0.2.0; host is 0.1.0",
        },
        {
          group: "Compatibility",
          message: "billing requires console package API 2; host supports 1",
        },
      ],
      module: {
        compatibility: {
          consolePackageApi: "2",
          lenso: {
            minVersion: "0.2.0",
          },
        },
        hostCompatibility: {
          consolePackageApi: "1",
          lensoVersion: "0.1.0",
        },
      },
    });
  });

  test("blocks registry review when provenance is incomplete", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: {
                publisher: "Lenso Fixtures",
              },
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "review",
          "billing",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      decision: "blocked",
      issues: [
        {
          group: "Provenance",
          message: "billing provenance source repository is missing",
        },
        {
          group: "Provenance",
          message: "billing provenance checksum is missing",
        },
        {
          group: "Provenance",
          message: "billing provenance signature URL is missing",
        },
        {
          group: "Provenance",
          message: "billing provenance public key id is missing",
        },
        {
          group: "Provenance",
          message: "billing provenance signature algorithm is missing",
        },
      ],
      module: {
        provenance: {
          publisher: "Lenso Fixtures",
        },
      },
    });
  });

  test("blocks registry review when signature algorithm is unsupported", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: {
                ...registryProvenanceForManifestUrl(manifestUrl),
                signatureAlgorithm: "rsa-pss",
              },
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "review",
          "billing",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      decision: "blocked",
      issues: [
        {
          group: "Provenance",
          message:
            "billing provenance signature algorithm rsa-pss is unsupported",
        },
      ],
    });
  });

  test("blocks registry review when signature verification fails", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    const wrongPublicKey = generateKeyPairSync("ed25519").publicKey.export({
      format: "pem",
      type: "spki",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-publishers.json",
      JSON.stringify(
        {
          publishers: [
            {
              publicKey: wrongPublicKey,
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
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: registryProvenanceForManifestUrl(manifestUrl),
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "review",
          "billing",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      decision: "blocked",
      issues: [
        {
          group: "Provenance",
          message: "billing provenance signature verification failed",
        },
      ],
    });
  });

  test("blocks registry review when publisher key is not trusted", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-publishers.json",
      JSON.stringify(
        {
          publishers: [
            {
              publicKey: registryPublicKeyPem,
              publicKeyId: "lenso-fixtures-ed25519",
              publisher: "Lenso Fixtures",
              status: "review_required",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: registryProvenanceForManifestUrl(manifestUrl),
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "review",
          "billing",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      decision: "blocked",
      issues: [
        {
          fix: 'lenso module publisher trust "Lenso Fixtures" "lenso-fixtures-ed25519" --public-key-file <pem>',
          group: "Provenance",
          message:
            "billing publisher key lenso-fixtures-ed25519 status is review_required",
        },
      ],
    });
  });

  test("blocks registry review when provenance checksum mismatches", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: {
                ...registryProvenanceForManifestUrl(manifestUrl),
                checksum:
                  "sha256:0000000000000000000000000000000000000000000000000000000000000000",
              },
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "review",
          "billing",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      decision: "blocked",
      issues: [
        {
          fix: `update billing provenance.checksum to ${registryProvenance.checksum} after reviewing the package artifact`,
          group: "Provenance",
          message: "billing provenance checksum mismatch",
        },
      ],
    });
  });

  test("rejects registry entries whose manifest identity does not match", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: [],
      console: [],
      name: "billing-pro",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              manifestReference: manifestUrl,
              name: "billing",
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    await expect(
      runConsolePackageCli([
        "module",
        "registry",
        "inspect",
        "billing",
        "--registry-file",
        path.join(repoRoot, ".lenso/module-registry.json"),
      ])
    ).rejects.toThrow(
      "Registry entry billing points to manifest for billing-pro"
    );
  });

  test("installs a remote module from a registry catalog", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [
        {
          package: {
            export: "billingConsoleModule",
            name: "@vendor/lenso-billing-console",
          },
          route: "/data/billing",
        },
      ],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    const baseUrl = manifestUrl.slice(0, -"/manifest".length);
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl,
              capabilities: ["billing.read"],
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: registryProvenanceForManifestUrl(manifestUrl),
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "install",
          "billing",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
          "--repo-root",
          repoRoot,
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Added remote module billing.");
    expect(logs).toContain("Installed registry module billing.");
    await expect(readFile(path.join(repoRoot, ".env"), "utf-8")).resolves.toBe(
      `REMOTE_MODULES=billing=${baseUrl}\n`
    );
    const installPlan = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/console-package-install-plan.json"),
        "utf-8"
      )
    );
    expect(installPlan).toMatchObject({
      modules: [
        {
          baseUrl,
          consolePackages: [
            {
              exportName: "billingConsoleModule",
              packageName: "@vendor/lenso-billing-console",
              requestedByModule: "billing",
              route: "/data/billing",
            },
          ],
          manifestReference: manifestUrl,
          moduleName: "billing",
        },
      ],
      version: 1,
    });
    const installHistory = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-registry-install-history.json"),
        "utf-8"
      )
    );
    expect(installHistory).toMatchObject({
      entries: [
        {
          action: "registry.install",
          baseUrl,
          catalogVersion: "0.1.0",
          consolePackageHints: 0,
          installPolicy: "trusted",
          manifestReference: manifestUrl,
          moduleName: "billing",
          provenance: registryProvenanceForManifestUrl(manifestUrl),
          source: "remote",
        },
      ],
      version: 1,
    });
    expect(installHistory.entries[0].installedAt).toEqual(expect.any(String));
  });

  test("rejects registry install entries without a trusted install policy", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              manifestReference: manifestUrl,
              name: "billing",
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    await expect(
      runConsolePackageCli([
        "module",
        "registry",
        "install",
        "billing",
        "--registry-file",
        path.join(repoRoot, ".lenso/module-registry.json"),
        "--repo-root",
        repoRoot,
      ])
    ).rejects.toThrow(
      "Registry module billing review is blocked before installation"
    );
  });

  test("rejects registry installs when review finds catalog issues", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              consolePackages: [
                {
                  exportName: "billingConsoleModule",
                  packageName: "@vendor/lenso-billing-console",
                  route: "/data/billing",
                },
              ],
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: registryProvenance,
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    await expect(
      runConsolePackageCli([
        "module",
        "registry",
        "install",
        "billing",
        "--registry-file",
        path.join(repoRoot, ".lenso/module-registry.json"),
        "--repo-root",
        repoRoot,
      ])
    ).rejects.toThrow(
      "Registry module billing review is blocked before installation"
    );
    await expect(
      readFile(path.join(repoRoot, ".env"), "utf-8")
    ).rejects.toThrow("ENOENT");
    await expect(
      readFile(
        path.join(repoRoot, ".lenso/module-registry-install-history.json"),
        "utf-8"
      )
    ).rejects.toThrow("ENOENT");
  });

  test("rejects registry installs when compatibility is blocked", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              compatibility: {
                lenso: {
                  minVersion: "0.2.0",
                },
              },
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: registryProvenance,
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    await expect(
      runConsolePackageCli([
        "module",
        "registry",
        "install",
        "billing",
        "--registry-file",
        path.join(repoRoot, ".lenso/module-registry.json"),
        "--repo-root",
        repoRoot,
      ])
    ).rejects.toThrow("billing requires Lenso >= 0.2.0; host is 0.1.0");
    await expect(
      readFile(path.join(repoRoot, ".env"), "utf-8")
    ).rejects.toThrow("ENOENT");
  });

  test("rejects registry installs when provenance is missing", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    await expect(
      runConsolePackageCli([
        "module",
        "registry",
        "install",
        "billing",
        "--registry-file",
        path.join(repoRoot, ".lenso/module-registry.json"),
        "--repo-root",
        repoRoot,
      ])
    ).rejects.toThrow("billing provenance publisher is missing");
    await expect(
      readFile(path.join(repoRoot, ".env"), "utf-8")
    ).rejects.toThrow("ENOENT");
  });

  test("prints registry install history entries", async () => {
    const repoRoot = await createRepoFixture();
    await writeRegistryInstallHistoryFixture(repoRoot);

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "history",
          "--repo-root",
          repoRoot,
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Module registry install history:");
    expect(logs).toContain("billing 0.1.0 install");
    expect(logs).toContain("action: registry.install");
    expect(logs).toContain("recorded: 2026-06-07T12:00:00.000Z");
    expect(logs).toContain("base URL: https://example.com/lenso/module/v1");
    expect(logs).toContain(
      "manifest: https://example.com/lenso/module/v1/manifest"
    );
  });

  test("prints registry install history as JSON", async () => {
    const repoRoot = await createRepoFixture();
    await writeRegistryInstallHistoryFixture(repoRoot);

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "history",
          "--repo-root",
          repoRoot,
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toEqual({
      count: 1,
      entries: [
        {
          action: "registry.install",
          baseUrl: "https://example.com/lenso/module/v1",
          catalogVersion: "0.1.0",
          consolePackageHints: 1,
          installPolicy: "trusted",
          installedAt: "2026-06-07T12:00:00.000Z",
          manifestReference: "https://example.com/lenso/module/v1/manifest",
          moduleName: "billing",
          provenance: registryProvenance,
          source: "remote",
        },
      ],
      historyFile: path.join(
        repoRoot,
        ".lenso/module-registry-install-history.json"
      ),
      version: 1,
    });
  });

  test("exports a marketplace bundle", async () => {
    const repoRoot = await createRepoFixture();
    await writeRegistryInstallHistoryFixture(repoRoot);
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              capabilities: ["billing.read"],
              installPolicy: "trusted",
              manifestReference: "https://example.com/lenso/module/v1/manifest",
              name: "billing",
              provenance: registryProvenance,
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "marketplace",
          "export",
          "--repo-root",
          repoRoot,
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Exported marketplace bundle.");
    expect(logs).toContain("- bundle: .lenso/marketplace-bundle.json");
    const bundle = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/marketplace-bundle.json"),
        "utf-8"
      )
    );
    expect(bundle).toMatchObject({
      history: {
        entries: [
          {
            action: "registry.install",
            moduleName: "billing",
          },
        ],
        historyFile: ".lenso/module-registry-install-history.json",
        version: 1,
      },
      publishers: {
        publishers: [
          {
            publicKeyId: "lenso-fixtures-ed25519",
            publisher: "Lenso Fixtures",
            status: "trusted",
          },
        ],
        publishersFile: ".lenso/module-publishers.json",
        version: 1,
      },
      registry: {
        modules: [
          {
            name: "billing",
            version: "0.1.0",
          },
        ],
        registryFile: ".lenso/module-registry.json",
        version: 1,
      },
      version: 1,
    });
    expect(bundle.exportedAt).toEqual(expect.any(String));
  });

  test("prints marketplace export summary as JSON", async () => {
    const repoRoot = await createRepoFixture();
    const outputFile = path.join(repoRoot, ".lenso/bundle.json");

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "marketplace",
          "export",
          "--repo-root",
          repoRoot,
          "--output-file",
          outputFile,
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toEqual({
      bundleFile: outputFile,
      historyEntries: 0,
      modules: 0,
      publishers: 1,
      version: 1,
    });
    await expect(readFile(outputFile, "utf-8")).resolves.toContain(
      '"version": 1'
    );
  });

  test("imports a marketplace bundle", async () => {
    const repoRoot = await createRepoFixture();
    const bundleFile = path.join(repoRoot, ".lenso/import-bundle.json");
    await writeFixture(
      repoRoot,
      ".lenso/import-bundle.json",
      JSON.stringify(
        {
          history: {
            entries: [],
            version: 1,
          },
          publishers: {
            publishers: [
              {
                publicKey: registryPublicKeyPem,
                publicKeyId: "acme-ed25519",
                publisher: "Acme Billing",
                status: "trusted",
              },
            ],
            version: 1,
          },
          registry: {
            modules: [
              {
                installPolicy: "review_required",
                manifestReference: "https://example.com/lenso/acme/v1/manifest",
                name: "acme-billing",
                source: "remote",
                version: "0.1.0",
              },
            ],
            version: 1,
          },
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "marketplace",
          "import",
          bundleFile,
          "--repo-root",
          repoRoot,
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toMatchObject({
      historyEntries: 0,
      modules: 1,
      publishers: 1,
      version: 1,
    });
    const registry = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-registry.json"),
        "utf-8"
      )
    );
    expect(registry.modules).toEqual([
      {
        capabilities: [],
        compatibility: {},
        consolePackages: [],
        installPolicy: "review_required",
        manifestReference: "https://example.com/lenso/acme/v1/manifest",
        name: "acme-billing",
        provenance: {},
        source: "remote",
        version: "0.1.0",
      },
    ]);
    const publishers = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-publishers.json"),
        "utf-8"
      )
    );
    expect(publishers.publishers).toEqual([
      {
        publicKey: registryPublicKeyPem,
        publicKeyId: "acme-ed25519",
        publisher: "Acme Billing",
        status: "trusted",
      },
      {
        notes: "Fixture publisher key",
        publicKey: registryPublicKeyPem,
        publicKeyId: "lenso-fixtures-ed25519",
        publisher: "Lenso Fixtures",
        status: "trusted",
      },
    ]);
  });

  test("imports marketplace bundle history when requested", async () => {
    const repoRoot = await createRepoFixture();
    await writeRegistryInstallHistoryFixture(repoRoot);
    const bundleFile = path.join(repoRoot, ".lenso/import-history-bundle.json");
    await writeFixture(
      repoRoot,
      ".lenso/import-history-bundle.json",
      JSON.stringify(
        {
          history: {
            entries: [
              {
                action: "registry.archive",
                archivedAt: "2026-06-07T13:00:00.000Z",
                catalogVersion: "0.2.0",
                manifestReference: "https://example.com/lenso/acme/v2/manifest",
                moduleName: "acme-billing",
                source: "remote",
              },
            ],
            version: 1,
          },
          publishers: {
            publishers: [],
            version: 1,
          },
          registry: {
            modules: [],
            version: 1,
          },
          version: 1,
        },
        null,
        2
      )
    );

    await expect(
      runConsolePackageCli([
        "module",
        "marketplace",
        "import",
        bundleFile,
        "--repo-root",
        repoRoot,
        "--include-history",
      ])
    ).resolves.toBe(0);

    const history = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/module-registry-install-history.json"),
        "utf-8"
      )
    );
    expect(history.entries).toHaveLength(2);
    expect(history.entries[1]).toMatchObject({
      action: "registry.archive",
      moduleName: "acme-billing",
    });
  });

  test("passes registry doctor for a valid catalog", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [
        {
          package: {
            export: "billingConsoleModule",
            name: "@vendor/lenso-billing-console",
          },
          route: "/data/billing",
        },
      ],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              capabilities: ["billing.read"],
              consolePackages: [
                {
                  exportName: "billingConsoleModule",
                  packageName: "@vendor/lenso-billing-console",
                  route: "/data/billing",
                },
              ],
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: registryProvenanceForManifestUrl(manifestUrl),
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "doctor",
          "--registry-file",
          path.join(repoRoot, ".lenso/module-registry.json"),
        ])
      ).resolves.toBe(0);
    });

    expect(logs).toContain("Module registry doctor passed.");
    expect(logs).toContain("catalog modules: 1");
    expect(logs).toContain("console package hints: 1");
  });

  test("prints registry doctor JSON snapshot for a valid catalog", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [
        {
          package: {
            export: "billingConsoleModule",
            name: "@vendor/lenso-billing-console",
          },
          route: "/data/billing",
        },
      ],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    const registryFile = path.join(repoRoot, ".lenso/module-registry.json");
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: manifestUrl.slice(0, -"/manifest".length),
              capabilities: ["billing.read"],
              consolePackages: [
                {
                  exportName: "billingConsoleModule",
                  packageName: "@vendor/lenso-billing-console",
                  route: "/data/billing",
                },
              ],
              installPolicy: "trusted",
              manifestReference: manifestUrl,
              name: "billing",
              provenance: registryProvenanceForManifestUrl(manifestUrl),
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    const logs = await captureConsoleLogs(async () => {
      await expect(
        runConsolePackageCli([
          "module",
          "registry",
          "doctor",
          "--registry-file",
          registryFile,
          "--json",
        ])
      ).resolves.toBe(0);
    });

    expect(JSON.parse(logs)).toEqual({
      catalog: {
        modules: 1,
        registryFile,
        version: 1,
      },
      issues: [],
      modules: [
        {
          baseUrl: manifestUrl.slice(0, -"/manifest".length),
          catalogVersion: "0.1.0",
          compatibility: {},
          consolePackageHints: 1,
          hostCompatibility: {
            consolePackageApi: "1",
            lensoVersion: "0.1.0",
          },
          installPolicy: "trusted",
          manifestName: "billing",
          manifestReference: manifestUrl,
          manifestStatus: "ok",
          manifestVersion: "0.1.0",
          name: "billing",
          provenance: registryProvenanceForManifestUrl(manifestUrl),
          publisherKey: {
            publicKeyId: "lenso-fixtures-ed25519",
            publisher: "Lenso Fixtures",
            status: "trusted",
          },
          source: "remote",
          status: "ready",
        },
      ],
      status: "passed",
      version: 1,
    });
  });

  test("groups registry doctor issues with fix commands", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      "billing-pro.module.json",
      JSON.stringify({
        capabilities: ["billing.read"],
        console: [
          {
            package: {
              export: "billingConsoleModule",
              name: "@vendor/lenso-billing-console",
            },
            route: "/data/billing",
          },
        ],
        name: "billing-pro",
        source: "remote",
        version: "0.2.0",
      })
    );
    await writeFixture(
      repoRoot,
      ".lenso/module-registry.json",
      JSON.stringify(
        {
          modules: [
            {
              consolePackages: [
                {
                  exportName: "missingConsoleModule",
                  packageName: "@vendor/missing-console",
                  route: "/data/missing",
                },
              ],
              manifestReference: path.join(repoRoot, "billing-pro.module.json"),
              name: "billing",
              source: "remote",
              version: "0.1.0",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    let message = "";
    try {
      await runConsolePackageCli([
        "module",
        "registry",
        "doctor",
        "--registry-file",
        path.join(repoRoot, ".lenso/module-registry.json"),
      ]);
    } catch (error) {
      ({ message } = error);
    }

    expect(message).toContain("Module registry doctor found");
    expect(message).toContain("Catalog");
    expect(message).toContain("billing baseUrl is missing");
    expect(message).toContain(
      "fix: add baseUrl or use a manifest URL ending with /manifest"
    );
    expect(message).toContain("Manifest");
    expect(message).toContain(
      "billing catalog name does not match manifest name billing-pro"
    );
    expect(message).toContain("Console package hint");
    expect(message).toContain(
      "@vendor/missing-console#missingConsoleModule is not declared by manifest billing-pro"
    );
  });

  test("groups module doctor issues with fix commands", async () => {
    const repoRoot = await createRepoFixture();
    await createRuntimeConsoleFixture(repoRoot);
    await writeFixture(
      repoRoot,
      ".lenso/console-package-install-plan.json",
      JSON.stringify(
        {
          modules: [
            {
              baseUrl: "http://127.0.0.1:4200/lenso/module/v1",
              consolePackages: [
                {
                  exportName: "billingConsoleModule",
                  packageName: "@vendor/lenso-billing-console",
                  requestedByModule: "billing",
                  route: "/data/billing",
                },
              ],
              moduleName: "billing",
            },
          ],
          version: 1,
        },
        null,
        2
      )
    );

    let message = "";
    try {
      await runConsolePackageCli(["module", "doctor", "--repo-root", repoRoot]);
    } catch (error) {
      ({ message } = error);
    }

    expect(message).toContain("Remote source");
    expect(message).toContain("REMOTE_MODULES is missing module billing");
    expect(message).toContain(
      "fix: lenso module add <manifest-url> --base-url http://127.0.0.1:4200/lenso/module/v1"
    );
    expect(message).toContain("Console package");
    expect(message).toContain(
      "Runtime Console dependency is missing: @vendor/lenso-billing-console"
    );
    expect(message).toContain(
      "fix: pnpm --dir apps/runtime-console add @vendor/lenso-billing-console"
    );
    expect(message).toContain("Registry mapping");
    expect(message).toContain(
      "fix: lenso console-package apply-plan --repo-root"
    );
  });

  test("documents the third-party remote module install flow", async () => {
    const runtimeConsoleRoot = path.resolve(import.meta.dirname, "../../..");
    const repoRoot = path.resolve(runtimeConsoleRoot, "../..");
    const flowDoc = await readFile(
      path.join(runtimeConsoleRoot, "docs/remote-module-install-flow.md"),
      "utf-8"
    );
    expect(flowDoc).toContain("lenso module add");
    expect(flowDoc).toContain("lenso module marketplace install");
    expect(flowDoc).toContain("## Advanced Hardening");
    expect(flowDoc).toContain("lenso module publisher list");
    expect(flowDoc).toContain("lenso module publisher doctor");
    expect(flowDoc).toContain("lenso module publisher trust");
    expect(flowDoc).toContain("lenso module publisher revoke");
    expect(flowDoc).toContain("lenso module registry add");
    expect(flowDoc).toContain("lenso module registry remove");
    expect(flowDoc).toContain("lenso module registry restore");
    expect(flowDoc).toContain("lenso module marketplace export");
    expect(flowDoc).toContain("lenso module marketplace import");
    expect(flowDoc).toContain("lenso module registry list");
    expect(flowDoc).toContain("lenso module registry doctor");
    expect(flowDoc).toContain("lenso module registry doctor --registry-file");
    expect(flowDoc).toContain("/admin/data/module-registry/snapshot");
    expect(flowDoc).toContain("Available Modules");
    expect(flowDoc).toContain("does not require publisher keys");
    expect(flowDoc).toContain("lenso module registry inspect");
    expect(flowDoc).toContain("lenso module registry review");
    expect(flowDoc).toContain("lenso module registry install");
    expect(flowDoc).toContain("lenso module registry history");
    expect(flowDoc).toContain("lenso console-package apply-plan");
    expect(flowDoc).toContain("lenso module doctor");
    expect(flowDoc).toContain("## Troubleshooting");
    expect(flowDoc).toContain("Remote source");
    expect(flowDoc).toContain("Console package");
    expect(flowDoc).toContain("Registry mapping");
    expect(flowDoc).toContain("fix: lenso module add <manifest-url>");
    expect(flowDoc).toContain(
      "fix: pnpm --dir apps/runtime-console add <package-name>"
    );
    expect(flowDoc).toContain("fix: lenso console-package apply-plan");
    expect(flowDoc).toContain("Remote module install demo passed");
    expect(flowDoc).toContain("Module registry install demo passed");

    await expect(
      readFile(path.join(repoRoot, "apps/runtime-console/README.md"), "utf-8")
    ).resolves.toContain("docs/remote-module-install-flow.md");
    await expect(
      readFile(
        path.join(repoRoot, "docs/architecture/third-party-modules.md"),
        "utf-8"
      )
    ).resolves.toContain(
      "apps/runtime-console/docs/remote-module-install-flow.md"
    );
  });

  test("keeps third-party module support status current", async () => {
    const repoRoot = path.resolve(
      path.resolve(import.meta.dirname, "../../.."),
      "../.."
    );
    const architectureDoc = await readFile(
      path.join(repoRoot, "docs/architecture/third-party-modules.md"),
      "utf-8"
    );

    expect(architectureDoc).toContain("remote module install CLI");
    expect(architectureDoc).toContain("lenso module marketplace install");
    expect(architectureDoc).toContain("advanced hardening tools");
    expect(architectureDoc).toContain("Module Registry v0");
    expect(architectureDoc).toContain("module publisher list");
    expect(architectureDoc).toContain("module publisher doctor");
    expect(architectureDoc).toContain("module publisher trust");
    expect(architectureDoc).toContain("module publisher revoke");
    expect(architectureDoc).toContain("module registry add");
    expect(architectureDoc).toContain("module registry remove`/`restore");
    expect(architectureDoc).toContain("module marketplace export");
    expect(architectureDoc).toContain("module marketplace import");
    expect(architectureDoc).toContain("module registry list");
    expect(architectureDoc).toContain("module registry doctor");
    expect(architectureDoc).toContain("module registry inspect");
    expect(architectureDoc).toContain("module registry review");
    expect(architectureDoc).toContain("module registry install");
    expect(architectureDoc).toContain("console package apply-plan");
    expect(architectureDoc).toContain("module doctor diagnostics");
    expect(architectureDoc.match(/embedded host bridges/gu) ?? []).toHaveLength(
      1
    );
  });

  test("shows the third-party remote module flow in CLI help", async () => {
    const { stdout } = await execFileAsync(process.execPath, [
      path.join(import.meta.dirname, "index.mjs"),
      "module",
      "--help",
    ]);

    expect(stdout).toContain("Remote module install");
    expect(stdout).toContain("lenso module add <manifest-url>");
    expect(stdout).toContain("lenso module marketplace install <manifest-url>");
    expect(stdout).toContain("Advanced registry and hardening");
    expect(stdout).toContain("lenso module publisher list");
    expect(stdout).toContain("lenso module publisher doctor");
    expect(stdout).toContain(
      "lenso module publisher trust <publisher> <public-key-id>"
    );
    expect(stdout).toContain(
      "lenso module publisher revoke <publisher> <public-key-id>"
    );
    expect(stdout).toContain("lenso module registry add <module>");
    expect(stdout).toContain("lenso module registry remove <module>");
    expect(stdout).toContain("lenso module registry restore <module>");
    expect(stdout).toContain("lenso module marketplace export");
    expect(stdout).toContain("lenso module marketplace import <bundle>");
    expect(stdout).toContain("lenso module registry list");
    expect(stdout).toContain("lenso module registry doctor");
    expect(stdout).toContain("lenso module registry install <module>");
    expect(stdout).toContain("lenso console-package apply-plan");
    expect(stdout).toContain("lenso module doctor");
  });

  test("runs the remote module install demo script", async () => {
    const runtimeConsoleRoot = path.resolve(import.meta.dirname, "../../..");
    const packageJson = JSON.parse(
      await readFile(path.join(runtimeConsoleRoot, "package.json"), "utf-8")
    );
    expect(packageJson.scripts["demo:remote-module-install"]).toBe(
      "node scripts/remote-module-install-demo.mjs"
    );
    expect(packageJson.scripts["demo:module-registry-install"]).toBe(
      "node scripts/module-registry-install-demo.mjs"
    );

    const { stdout } = await execFileAsync(process.execPath, [
      path.join(runtimeConsoleRoot, "scripts/remote-module-install-demo.mjs"),
    ]);

    expect(stdout).toContain("Remote module install demo passed");
    expect(stdout).toContain("Module doctor passed");
  });

  test("runs the module registry install demo script", async () => {
    const runtimeConsoleRoot = path.resolve(import.meta.dirname, "../../..");
    const { stdout } = await execFileAsync(process.execPath, [
      path.join(runtimeConsoleRoot, "scripts/module-registry-install-demo.mjs"),
    ]);

    expect(stdout).toContain("Module registry entries:");
    expect(stdout).toContain("Module registry doctor passed.");
    expect(stdout).toContain("Registry module billing");
    expect(stdout).toContain("Installed registry module billing.");
    expect(stdout).toContain("Module doctor passed");
    expect(stdout).toContain("Module registry install demo passed");
  });

  test("creates a standalone remote module package", async () => {
    const outputRoot = await mkdtemp(
      path.join(os.tmpdir(), "lenso-remote-module-cli-")
    );
    tempRoots.push(outputRoot);

    await expect(
      runConsolePackageCli([
        "module",
        "create",
        "billing",
        "--remote",
        "--output-dir",
        outputRoot,
      ])
    ).resolves.toBe(0);

    const packageRoot = path.join(outputRoot, "lenso-billing");
    const manifest = JSON.parse(
      await readFile(path.join(packageRoot, "lenso.module.json"), "utf-8")
    );
    expect(manifest).toMatchObject({
      capabilities: ["billing.read"],
      console: [
        {
          navigation: {
            order: 10,
            workspace: {
              icon: "database",
              id: "billing",
              label: "Billing",
            },
          },
          package: {
            export: "billingConsoleModule",
            name: "@vendor/lenso-billing-console",
          },
          route: "/data/billing",
        },
      ],
      http_routes: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });
    const consoleSurface = JSON.parse(
      await readFile(
        path.join(packageRoot, "console/console-surface.json"),
        "utf-8"
      )
    );
    expect(consoleSurface).toMatchObject({
      navigation: {
        order: 10,
        workspace: {
          icon: "database",
          id: "billing",
          label: "Billing",
        },
      },
    });

    await expect(
      readFile(path.join(packageRoot, "backend/README.md"), "utf-8")
    ).resolves.toContain("Remote module backend");
    const consolePackageJson = JSON.parse(
      await readFile(path.join(packageRoot, "console/package.json"), "utf-8")
    );
    expect(consolePackageJson).toMatchObject({
      name: "@vendor/lenso-billing-console",
      peerDependencies: {
        "@lenso/runtime-console-api": "^0.1.0",
      },
      private: false,
    });
    const consoleSource = await readFile(
      path.join(packageRoot, "console/src/index.tsx"),
      "utf-8"
    );
    expect(consoleSource).toContain("billingConsoleModule");
    expect(consoleSource).toContain(
      "navigation: billingConsoleManifest.navigation"
    );
    await expect(
      readFile(path.join(packageRoot, "contracts/README.md"), "utf-8")
    ).resolves.toContain("Module-owned contracts");
    await expect(
      readFile(path.join(packageRoot, "README.md"), "utf-8")
    ).resolves.toContain("lenso module add");
  });

  test("adds a remote module source from a manifest to an env file", async () => {
    const repoRoot = await createRepoFixture();
    const manifestPath = path.join(repoRoot, "billing.module.json");
    await writeFixture(
      repoRoot,
      "billing.module.json",
      JSON.stringify(
        {
          capabilities: ["billing.read"],
          console: [
            {
              package: {
                export: "billingConsoleModule",
                name: "@vendor/lenso-billing-console",
              },
              route: "/data/billing",
            },
          ],
          name: "billing",
          source: "remote",
          version: "0.1.0",
        },
        null,
        2
      )
    );
    await writeFixture(
      repoRoot,
      ".env",
      "APP_ENV=local\nREMOTE_MODULES=remote-crm=http://127.0.0.1:4100/lenso/module/v1\nRUST_LOG=info\n"
    );

    await expect(
      runConsolePackageCli([
        "module",
        "add",
        pathToFileURL(manifestPath).href,
        "--repo-root",
        repoRoot,
        "--base-url",
        "http://127.0.0.1:4200/lenso/module/v1",
      ])
    ).resolves.toBe(0);

    const envFile = await readFile(path.join(repoRoot, ".env"), "utf-8");
    expect(envFile).toContain(
      "REMOTE_MODULES=remote-crm=http://127.0.0.1:4100/lenso/module/v1,billing=http://127.0.0.1:4200/lenso/module/v1"
    );
    expect(envFile).toContain("RUST_LOG=info");

    const installPlan = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/console-package-install-plan.json"),
        "utf-8"
      )
    );
    expect(installPlan).toMatchObject({
      modules: [
        {
          baseUrl: "http://127.0.0.1:4200/lenso/module/v1",
          consolePackages: [
            {
              command:
                "pnpm --dir apps/runtime-console add @vendor/lenso-billing-console",
              exportName: "billingConsoleModule",
              key: "@vendor/lenso-billing-console#billingConsoleModule",
              packageName: "@vendor/lenso-billing-console",
              requestedByModule: "billing",
              route: "/data/billing",
            },
          ],
          moduleName: "billing",
        },
      ],
      version: 1,
    });
  });

  test("derives the remote base url from protocol manifest urls", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });

    await expect(
      runConsolePackageCli([
        "module",
        "add",
        manifestUrl,
        "--repo-root",
        repoRoot,
      ])
    ).resolves.toBe(0);

    await expect(readFile(path.join(repoRoot, ".env"), "utf-8")).resolves.toBe(
      `REMOTE_MODULES=billing=${manifestUrl.slice(0, -"/manifest".length)}\n`
    );
  });

  test("installs a marketplace module from a manifest url", async () => {
    const repoRoot = await createRepoFixture();
    const manifestUrl = await serveManifest({
      capabilities: ["billing.read"],
      console: [],
      name: "billing",
      source: "remote",
      version: "0.1.0",
    });

    await expect(
      runConsolePackageCli([
        "module",
        "marketplace",
        "install",
        manifestUrl,
        "--repo-root",
        repoRoot,
      ])
    ).resolves.toBe(0);

    await expect(readFile(path.join(repoRoot, ".env"), "utf-8")).resolves.toBe(
      `REMOTE_MODULES=billing=${manifestUrl.slice(0, -"/manifest".length)}\n`
    );
  });

  test("keeps remote module source installation idempotent", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      "billing.module.json",
      JSON.stringify({
        capabilities: ["billing.read"],
        console: [
          {
            package: {
              export: "billingConsoleModule",
              name: "@vendor/lenso-billing-console",
            },
            route: "/data/billing",
          },
        ],
        name: "billing",
        source: "remote",
        version: "0.1.0",
      })
    );
    await writeFixture(
      repoRoot,
      ".env",
      "REMOTE_MODULES=billing=http://127.0.0.1:4200/lenso/module/v1\n"
    );

    for (let index = 0; index < 2; index += 1) {
      await expect(
        runConsolePackageCli([
          "module",
          "add",
          path.join(repoRoot, "billing.module.json"),
          "--repo-root",
          repoRoot,
          "--base-url",
          "http://127.0.0.1:4200/lenso/module/v1",
        ])
      ).resolves.toBe(0);
    }

    await expect(readFile(path.join(repoRoot, ".env"), "utf-8")).resolves.toBe(
      "REMOTE_MODULES=billing=http://127.0.0.1:4200/lenso/module/v1\n"
    );

    const installPlan = JSON.parse(
      await readFile(
        path.join(repoRoot, ".lenso/console-package-install-plan.json"),
        "utf-8"
      )
    );
    expect(installPlan.modules).toHaveLength(1);
    expect(installPlan.modules[0].consolePackages).toHaveLength(1);
  });

  test("rejects non-remote module manifests during install", async () => {
    const repoRoot = await createRepoFixture();
    await writeFixture(
      repoRoot,
      "linked.module.json",
      JSON.stringify({
        capabilities: [],
        console: [],
        name: "billing",
        source: "linked",
        version: "0.1.0",
      })
    );

    await expect(
      runConsolePackageCli([
        "module",
        "add",
        path.join(repoRoot, "linked.module.json"),
        "--repo-root",
        repoRoot,
        "--base-url",
        "http://127.0.0.1:4200/lenso/module/v1",
      ])
    ).rejects.toThrow("Remote module manifest source must be remote");
  });
});
