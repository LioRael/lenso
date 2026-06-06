import { mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { afterEach, describe, expect, test } from "vitest";

import { runConsolePackageCli } from "./index.mjs";

const tempRoots = [];

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

const writeFixture = async (repoRoot, relativePath, contents) => {
  const { mkdir, writeFile } = await import("node:fs/promises");
  const filePath = path.join(repoRoot, relativePath);
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, contents);
};

afterEach(async () => {
  await Promise.all(
    tempRoots
      .splice(0)
      .map((root) => rm(root, { force: true, recursive: true }))
  );
});

describe("module scaffold CLI", () => {
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
});
