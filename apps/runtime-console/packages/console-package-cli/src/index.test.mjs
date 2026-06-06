import { execFile } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { promisify } from "node:util";

import { afterEach, describe, expect, test } from "vitest";

import { runConsolePackageCli } from "./index.mjs";

const tempRoots = [];
const tempServers = [];
const execFileAsync = promisify(execFile);

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
    response.statusCode = 404;
    response.end("not found");
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  tempServers.push(server);
  const { port } = server.address();
  return `http://127.0.0.1:${port}/lenso/module/v1/manifest`;
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

    await expect(
      readFile(
        path.join(
          repoRoot,
          "apps/runtime-console/packages/billing-console/src/index.tsx"
        ),
        "utf-8"
      )
    ).resolves.toContain("billingConsoleModule");
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

  test("shows the third-party remote module flow in CLI help", async () => {
    const { stdout } = await execFileAsync(process.execPath, [
      path.join(import.meta.dirname, "index.mjs"),
      "module",
      "--help",
    ]);

    expect(stdout).toContain("Third-party remote module flow");
    expect(stdout).toContain("lenso module add <manifest-url>");
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

    const { stdout } = await execFileAsync(process.execPath, [
      path.join(runtimeConsoleRoot, "scripts/remote-module-install-demo.mjs"),
    ]);

    expect(stdout).toContain("Remote module install demo passed");
    expect(stdout).toContain("Module doctor passed");
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
    await expect(
      readFile(path.join(packageRoot, "console/src/index.tsx"), "utf-8")
    ).resolves.toContain("billingConsoleModule");
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
