import { execFile, spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { promisify } from "node:util";

import { afterEach, describe, expect, test, vi } from "vitest";

import { runConsolePackageCli } from "./index.mjs";

const tempRoots = [];
const tempServers = [];
const tempProcesses = [];
const execFileAsync = promisify(execFile);

const readManifestUrlFromProcess = async (childProcess) => {
  const timeout = setTimeout(() => childProcess.kill(), 3000);
  try {
    for await (const chunk of childProcess.stdout) {
      const text = String(chunk);
      const manifestUrl = text.match(/http:\/\/\S+/u)?.[0];
      if (manifestUrl) {
        return manifestUrl;
      }
    }
  } finally {
    clearTimeout(timeout);
  }
  throw new Error("backend server did not print manifest URL");
};

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
  const { mkdir, writeFile: writeFixtureFile } =
    await import("node:fs/promises");
  const filePath = path.join(repoRoot, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFixtureFile(filePath, contents);
};

afterEach(async () => {
  for (const childProcess of tempProcesses.splice(0)) {
    childProcess.kill();
    await once(childProcess, "exit").catch(() => {
      /* empty */
    });
  }
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

  test("documents the third-party remote module install flow", async () => {
    const runtimeConsoleRoot = path.resolve(import.meta.dirname, "../../..");
    const repoRoot = path.resolve(runtimeConsoleRoot, "../..");
    const flowDoc = await readFile(
      path.join(runtimeConsoleRoot, "docs/remote-module-install-flow.md"),
      "utf-8"
    );
    expect(flowDoc).toContain("lenso module add");
    expect(flowDoc).toContain("lenso module marketplace install");
    expect(flowDoc).not.toContain("## Advanced Hardening");
    expect(flowDoc).not.toContain("lenso module publisher");
    expect(flowDoc).not.toContain("lenso module marketplace export");
    expect(flowDoc).not.toContain("lenso module marketplace import");
    expect(flowDoc).not.toContain("lenso module registry doctor");
    expect(flowDoc).toContain("/admin/data/available-modules");
    expect(flowDoc).toContain("Available Modules");
    expect(flowDoc).toContain("Installing from a manifest URL");
    expect(flowDoc).not.toContain("lenso module registry inspect");
    expect(flowDoc).not.toContain("lenso module registry review");
    expect(flowDoc).not.toContain("lenso module registry install");
    expect(flowDoc).not.toContain("lenso module registry history");
    expect(flowDoc).toContain("lenso console-package apply-plan");
    expect(flowDoc).not.toContain("lenso module doctor");
    expect(flowDoc).toContain("## Troubleshooting");
    expect(flowDoc).toContain("Remote source");
    expect(flowDoc).toContain("Console package");
    expect(flowDoc).toContain("Console registration");
    expect(flowDoc).toContain("fix: lenso module add <manifest-url>");
    expect(flowDoc).toContain(
      "fix: pnpm --dir apps/runtime-console add <package-name>"
    );
    expect(flowDoc).toContain("fix: lenso console-package apply-plan");
    expect(flowDoc).toContain("Remote module package demo passed");
    expect(flowDoc).not.toContain("Module registry install demo passed");

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
    expect(architectureDoc).not.toContain("advanced hardening tools");
    expect(architectureDoc).not.toContain("module publisher");
    expect(architectureDoc).not.toContain("module registry doctor");
    expect(architectureDoc).not.toContain("module marketplace export");
    expect(architectureDoc).not.toContain("module marketplace import");
    expect(architectureDoc).not.toContain("module registry review");
    expect(architectureDoc).not.toContain("module registry install");
    expect(architectureDoc).toContain("console package apply-plan");
    expect(architectureDoc).not.toContain("module doctor diagnostics");
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
    expect(stdout).not.toContain("Advanced registry and hardening");
    expect(stdout).not.toContain("lenso module publisher");
    expect(stdout).not.toContain("lenso module registry");
    expect(stdout).not.toContain("lenso module marketplace export");
    expect(stdout).not.toContain("lenso module marketplace import");
    expect(stdout).toContain("lenso console-package apply-plan");
    expect(stdout).not.toContain("lenso module doctor");
  });

  test("runs the remote module package demo script", async () => {
    const runtimeConsoleRoot = path.resolve(import.meta.dirname, "../../..");
    const packageJson = JSON.parse(
      await readFile(path.join(runtimeConsoleRoot, "package.json"), "utf-8")
    );
    expect(packageJson.scripts["demo:remote-module-install"]).toBe(
      "node scripts/remote-module-install-demo.mjs"
    );
    expect(packageJson.scripts["demo:remote-module-package"]).toBe(
      "node scripts/remote-module-package-demo.mjs"
    );
    expect(packageJson.scripts["demo:remote-module-run"]).toBe(
      "node scripts/remote-module-run-demo.mjs"
    );

    const { stdout } = await execFileAsync(process.execPath, [
      path.join(runtimeConsoleRoot, "scripts/remote-module-run-demo.mjs"),
    ]);

    expect(stdout).toContain("Remote module package demo passed");
    expect(stdout).toContain("Remote module install-to-run demo passed");
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
    const catalogEntry = JSON.parse(
      await readFile(path.join(packageRoot, "catalog-entry.json"), "utf-8")
    );
    const rootPackageJson = JSON.parse(
      await readFile(path.join(packageRoot, "package.json"), "utf-8")
    );
    expect(rootPackageJson).toMatchObject({
      name: "lenso-billing",
      private: true,
      scripts: {
        check: "pnpm --dir backend check && pnpm --dir console check",
        dev: "pnpm --dir backend dev",
        smoke: "pnpm --dir backend smoke",
      },
    });
    expect(manifest).toMatchObject({
      admin: {
        entities: [
          {
            fields: [
              { field_type: { kind: "string" }, name: "email" },
              { field_type: { kind: "string" }, name: "name" },
              { field_type: { kind: "timestamp" }, name: "created_at" },
            ],
            name: "contacts",
            read_capability: "billing.read",
          },
        ],
        kind: "schema",
      },
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
      http_routes: [
        {
          capability: "billing.read",
          display_name: "Fetch Contact",
          method: "GET",
          path: "/contacts/{id}",
          story_title: "Fetch Contact",
        },
      ],
      name: "billing",
      runtime: {
        functions: [
          {
            input_schema: "billing.contacts.enrich.v1",
            name: "billing.contacts.enrich.v1",
            queue: "billing",
            retry_policy: {
              initial_delay_ms: 1000,
              max_attempts: 3,
            },
            version: 1,
          },
        ],
      },
      source: "remote",
      version: "0.1.0",
    });
    expect(catalogEntry).toMatchObject({
      baseUrl: "https://example.com/lenso/module/v1",
      consolePackages: [
        {
          exportName: "billingConsoleModule",
          packageName: "@vendor/lenso-billing-console",
          route: "/data/billing",
        },
      ],
      manifestReference: "https://example.com/lenso/module/v1/manifest",
      name: manifest.name,
      source: manifest.source,
      summary: "Billing workspace and operations",
      version: manifest.version,
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
    ).resolves.toContain("pnpm dev");
    const backendPackageJson = JSON.parse(
      await readFile(path.join(packageRoot, "backend/package.json"), "utf-8")
    );
    expect(backendPackageJson).toMatchObject({
      dependencies: {
        "@lenso/remote-module-kit": "^0.1.0",
      },
      name: "billing-remote-backend",
      private: true,
      scripts: {
        check: "node src/smoke.mjs",
        dev: "node src/server.mjs",
        smoke: "node src/smoke.mjs",
        start: "node src/server.mjs",
      },
    });
    const backendServer = await readFile(
      path.join(packageRoot, "backend/src/server.mjs"),
      "utf-8"
    );
    expect(backendServer).toContain("defineRemoteModule");
    expect(backendServer).toContain("defineSchemaEntity");
    expect(backendServer).toContain("getRoute");
    expect(backendServer).toContain("runtimeFunction");
    expect(backendServer).toContain("serveRemoteModule");
    expect(backendServer).toContain('"GET /contacts/{id}"');
    expect(backendServer).toContain('"billing.contacts.enrich.v1"');
    expect(backendServer).toContain("contacts.slice(0, limit)");
    await expect(
      readFile(path.join(packageRoot, "backend/src/smoke.mjs"), "utf-8")
    ).resolves.toContain("backend smoke passed");
    backendPackageJson.dependencies["@lenso/remote-module-kit"] =
      `file:${path.resolve(import.meta.dirname, "../../remote-module-kit")}`;
    await writeFile(
      path.join(packageRoot, "backend/package.json"),
      `${JSON.stringify(backendPackageJson, null, 2)}\n`
    );
    await execFileAsync("pnpm", [
      "--dir",
      path.join(packageRoot, "backend"),
      "install",
    ]);
    const backendProcess = spawn(process.execPath, ["src/server.mjs"], {
      cwd: path.join(packageRoot, "backend"),
      env: { ...process.env, PORT: "0" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    tempProcesses.push(backendProcess);
    const manifestUrl = await readManifestUrlFromProcess(backendProcess);
    expect(manifestUrl).toContain("/lenso/module/v1/manifest");
    const servedManifest = await fetch(manifestUrl).then((response) =>
      response.json()
    );
    expect(servedManifest).toMatchObject({
      admin: {
        kind: "schema",
      },
      name: "billing",
      source: "remote",
    });
    await expect(
      fetch(`${manifestUrl.slice(0, -"/manifest".length)}/admin/contacts`).then(
        (response) => response.json()
      )
    ).resolves.toEqual(
      expect.objectContaining({
        records: expect.arrayContaining([
          expect.objectContaining({
            email: "ada@example.com",
            id: "contact_1",
          }),
        ]),
      })
    );
    await expect(
      fetch(
        `${manifestUrl.slice(0, -"/manifest".length)}/contacts/contact_1`
      ).then((response) => response.json())
    ).resolves.toMatchObject({
      email: "ada@example.com",
      id: "contact_1",
    });
    await expect(
      fetch(
        `${manifestUrl.slice(0, -"/manifest".length)}/runtime/functions/billing.contacts.enrich.v1/invoke`,
        {
          body: JSON.stringify({
            actor: { id: "worker", kind: "service", scopes: [] },
            attempt: 1,
            correlation_id: "corr_1",
            function_name: "billing.contacts.enrich.v1",
            function_run_id: "fnrun_1",
            input: { contact_id: "contact_1" },
            request_id: "req_1",
            trace: { span_id: "span_1", trace_id: "trace_1" },
          }),
          headers: { "content-type": "application/json" },
          method: "POST",
        }
      ).then((response) => response.json())
    ).resolves.toMatchObject({
      output: {
        contact: {
          email: "ada@example.com",
          id: "contact_1",
        },
        enriched: true,
      },
    });
    const consolePackageJson = JSON.parse(
      await readFile(path.join(packageRoot, "console/package.json"), "utf-8")
    );
    expect(consolePackageJson).toMatchObject({
      name: "@vendor/lenso-billing-console",
      peerDependencies: {
        "@lenso/runtime-console-api": "^0.1.0",
      },
      private: false,
      scripts: {
        check: "pnpm test && pnpm typecheck",
        test: 'echo "console package smoke passed"',
      },
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
    ).resolves.toContain("catalog-entry.json");
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

    const logs = [];
    const logSpy = vi.spyOn(console, "log").mockImplementation((message) => {
      logs.push(String(message));
    });
    try {
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
    } finally {
      logSpy.mockRestore();
    }

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
          restartRequired: true,
        },
      ],
      version: 1,
    });
    const output = logs.join("\n");
    expect(output).toContain("Added remote module billing.");
    expect(output).toContain("Updated:");
    expect(output).toContain("- .env");
    expect(output).toContain("- .lenso/console-package-install-plan.json");
    expect(output).toContain("Next steps:");
    expect(output).toContain("- lenso console-package apply-plan");
    expect(output).toContain("- pnpm --dir apps/runtime-console install");
    expect(output).toContain("- restart the API and worker");
    expect(output).not.toContain("review");
    expect(output).not.toContain("doctor");
    expect(output).not.toContain("hardening");
    expect(output).not.toContain("publisher");
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

  test("adds a remote module manifest to a local catalog", async () => {
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
    const logs = [];
    const logSpy = vi.spyOn(console, "log").mockImplementation((message) => {
      logs.push(String(message));
    });
    try {
      await expect(
        runConsolePackageCli([
          "module",
          "catalog",
          "add",
          manifestUrl,
          "--repo-root",
          repoRoot,
          "--summary",
          "Billing module",
        ])
      ).resolves.toBe(0);
    } finally {
      logSpy.mockRestore();
    }

    const catalog = JSON.parse(
      await readFile(path.join(repoRoot, ".lenso/module-catalog.json"), "utf-8")
    );
    expect(catalog).toEqual({
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
          manifestReference: manifestUrl,
          name: "billing",
          source: "remote",
          summary: "Billing module",
          version: "0.1.0",
        },
      ],
      version: 1,
    });
    const output = logs.join("\n");
    expect(output).toContain("Added billing to module catalog.");
    expect(output).toContain("- .lenso/module-catalog.json");
    expect(output).toContain(`- lenso module add ${manifestUrl}`);
    expect(output).not.toContain("review");
    expect(output).not.toContain("doctor");
    expect(output).not.toContain("publisher");
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
