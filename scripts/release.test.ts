import { expect, test } from "bun:test";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  releaseOrder,
  validateArchive,
  type Manifest,
  type ReleasePackage,
} from "./release-verify";

const core: Manifest = { name: "lenso", version: "0.1.0", exports: { ".": "./dist/index.js" } };
const entries = ["package/package.json", "package/dist/index.js", "package/dist/index.d.ts"];
const pkg = (manifest: Manifest): ReleasePackage => ({ directory: manifest.name, manifest });

async function run(cwd: string, command: string[]) {
  const child = Bun.spawn(command, { cwd, stdout: "pipe", stderr: "pipe", stdin: "ignore" });
  const [code, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  expect(code, `${command.join(" ")}\n${stdout}\n${stderr}`).toBe(0);
  return stdout;
}

test("dependency-first order uses package names, excludes private packages", () => {
  expect(
    releaseOrder([
      pkg({
        name: "lenso-cli",
        version: "0.1.0",
        dependencies: { "@lenso/engine": "workspace:^" },
      }),
      pkg({ name: "app", version: "0.1.0", private: true }),
      pkg({ name: "@lenso/engine", version: "0.1.0", dependencies: { lenso: "workspace:^" } }),
      pkg(core),
    ]).map((p) => p.manifest.name),
  ).toEqual(["lenso", "@lenso/engine", "lenso-cli"]);
});

test("unresolved, duplicate, private runtime and cyclic dependencies fail", () => {
  expect(() => releaseOrder([pkg({ ...core, dependencies: { absent: "workspace:^" } })])).toThrow(
    "unresolved",
  );
  expect(() => releaseOrder([pkg(core), pkg(core)])).toThrow("Duplicate");
  const hidden = { name: "hidden", version: "0.1.0", private: true };
  expect(() =>
    releaseOrder([pkg(hidden), pkg({ ...core, dependencies: { hidden: "*" } })]),
  ).toThrow("private");
  expect(() => releaseOrder([pkg({ ...core, dependencies: { lenso: "*" } })])).toThrow("cycle");
});

test("valid packed metadata, wildcard exports and CLI entry points pass", () => {
  validateArchive(core, core, entries);
  const manifest = {
    ...core,
    exports: { "./migrations/*": "./migrations/*" },
    bin: { lenso: "./dist/bin.js" },
  };
  validateArchive(manifest, manifest, [
    ...entries,
    "package/migrations/sqlite/001.sql",
    "package/dist/bin.js",
  ]);
});

test("missing artifacts, identity changes and leaked local references fail", () => {
  expect(() => validateArchive(core, { ...core, version: "0.2.0" }, entries)).toThrow("identity");
  expect(() => validateArchive(core, core, ["package/package.json"])).toThrow("dist");
  expect(() =>
    validateArchive(
      core,
      core,
      entries.filter((p) => !p.endsWith("index.js")),
    ),
  ).toThrow("entry point");
  for (const group of ["dependencies", "optionalDependencies", "peerDependencies"]) {
    for (const range of ["workspace:^", "file:../local", "link:../local"]) {
      expect(() => validateArchive(core, { ...core, [group]: { lenso: range } }, entries)).toThrow(
        "still uses",
      );
    }
  }
});

test("credentials, generated app files and nested tarballs fail", () => {
  for (const path of [
    ".npmrc",
    ".env",
    ".env.local",
    "examples/.lenso/client.ts",
    "vendor/x.tgz",
    "../escape",
    "node_modules/pkg/index.js",
  ]) {
    expect(() => validateArchive(core, core, [...entries, `package/${path}`])).toThrow("forbidden");
  }
});

test("publish-like flags are rejected before building or packing", async () => {
  const child = Bun.spawn([process.execPath, `${import.meta.dir}/release-verify.ts`, "--publish"], {
    stdout: "pipe",
    stderr: "pipe",
  });
  expect(await child.exited).toBe(1);
  expect(await new Response(child.stderr).text()).toContain("local verification only");
  expect(await new Response(child.stdout).text()).toBe("");
});

test("real Bun pack replaces workspace ranges and contains every public entry point", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-release-test-"));
  try {
    await mkdir(join(root, "packages/core/dist"), { recursive: true });
    await mkdir(join(root, "packages/cli/dist"), { recursive: true });
    await Bun.write(
      join(root, "package.json"),
      JSON.stringify({ private: true, workspaces: ["packages/*"] }),
    );
    await Bun.write(join(root, "packages/core/package.json"), JSON.stringify(core));
    const manifest: Manifest = {
      name: "release-fixture-cli",
      version: "0.2.0",
      files: ["dist"],
      exports: { ".": { types: "./dist/index.d.ts", import: "./dist/index.js" } },
      bin: { fixture: "./dist/bin.js" },
      dependencies: { lenso: "workspace:^" },
      peerDependencies: { lenso: "workspace:~" },
    };
    await Bun.write(join(root, "packages/cli/package.json"), JSON.stringify(manifest));
    await Bun.write(join(root, "packages/cli/dist/index.js"), "export const value = 1;");
    await Bun.write(join(root, "packages/cli/dist/index.d.ts"), "export declare const value: 1;");
    await Bun.write(
      join(root, "packages/cli/dist/bin.js"),
      "#!/usr/bin/env bun\nconsole.log('fixture');",
    );
    const archive = join(root, "fixture.tgz");
    await run(root, [process.execPath, "install", "--ignore-scripts"]);
    await run(join(root, "packages/cli"), [
      process.execPath,
      "pm",
      "pack",
      "--ignore-scripts",
      "--filename",
      archive,
    ]);
    const packed = JSON.parse(await run(root, ["tar", "-xOzf", archive, "package/package.json"]));
    expect(packed.dependencies.lenso).toBe("^0.1.0");
    expect(packed.peerDependencies.lenso).toBe("~0.1.0");
    const actualEntries = (await run(root, ["tar", "-tzf", archive])).trim().split("\n");
    validateArchive(manifest, packed, actualEntries);
    expect(actualEntries).toContain("package/dist/bin.js");
    expect(actualEntries).toContain("package/dist/index.d.ts");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("installed Changesets plans and versions a Bun workspace without publishing", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-changesets-test-"));
  const cli = join(import.meta.dir, "../node_modules/@changesets/cli/bin.js");
  const config = await Bun.file(join(import.meta.dir, "../.changeset/config.json")).json();
  const manifests = {
    core: { name: "fixture-core", version: "1.0.0" },
    consumer: {
      name: "fixture-consumer",
      version: "1.0.0",
      dependencies: { "fixture-core": "workspace:^" },
    },
    peer: {
      name: "fixture-peer",
      version: "1.0.0",
      peerDependencies: { "fixture-core": "^1.0.0" },
    },
    independent: { name: "fixture-independent", version: "7.0.0" },
    hidden: {
      name: "fixture-hidden",
      version: "1.0.0",
      private: true,
      dependencies: { "fixture-core": "workspace:^" },
    },
  };
  try {
    await run(root, ["git", "init", "--initial-branch=main"]);
    await run(root, [
      "env",
      "GIT_EDITOR=true",
      "git",
      "-c",
      "user.name=Release Fixture",
      "-c",
      "user.email=fixture@example.invalid",
      "-c",
      "commit.gpgsign=false",
      "commit",
      "--allow-empty",
      "-m",
      "Initialize isolated release fixture",
    ]);
    await Bun.write(
      join(root, "package.json"),
      JSON.stringify({ private: true, version: "1.0.0", workspaces: ["packages/*", "examples/*"] }),
    );
    await Bun.write(join(root, ".changeset/config.json"), JSON.stringify(config));
    for (const [directory, manifest] of Object.entries(manifests)) {
      await Bun.write(join(root, `packages/${directory}/package.json`), JSON.stringify(manifest));
    }
    await Bun.write(
      join(root, "examples/demo/package.json"),
      JSON.stringify({
        name: "fixture-example",
        version: "1.0.0",
        private: true,
        dependencies: { "fixture-core": "workspace:^" },
      }),
    );
    for (const directory of ["templates/demo", "packages/core/examples/plugin"]) {
      await Bun.write(
        join(root, directory, "package.json"),
        JSON.stringify({ name: `outside-${directory.replaceAll("/", "-")}`, version: "1.0.0" }),
      );
    }
    await Bun.write(
      join(root, ".changeset/core-major.md"),
      '---\n"fixture-core": major\n---\n\nChange the fixture core contract.\n',
    );
    await run(root, [process.execPath, "install", "--ignore-scripts"]);
    await run(root, [process.execPath, cli, "add", "--empty"]);
    await run(root, [process.execPath, cli, "status", "--output", "status.json"]);
    const plan = await Bun.file(join(root, "status.json")).json();
    expect(
      plan.releases
        .filter((release: { type: string }) => release.type !== "none")
        .map((release: { name: string }) => release.name)
        .sort(),
    ).toEqual(["fixture-consumer", "fixture-core", "fixture-peer"]);
    for (const name of ["fixture-hidden", "fixture-example"]) {
      expect(
        plan.releases.find((release: { name: string }) => release.name === name),
      ).toMatchObject({ type: "none", newVersion: "1.0.0" });
    }
    expect((await Bun.file(join(root, "packages/core/package.json")).json()).version).toBe("1.0.0");

    await run(root, [process.execPath, cli, "version"]);
    const coreManifest = await Bun.file(join(root, "packages/core/package.json")).json();
    const consumer = await Bun.file(join(root, "packages/consumer/package.json")).json();
    const peer = await Bun.file(join(root, "packages/peer/package.json")).json();
    expect(coreManifest.version).toBe("2.0.0");
    expect(consumer.version).toBe("1.0.1");
    expect(consumer.dependencies["fixture-core"]).toBe("workspace:^");
    expect(peer.peerDependencies["fixture-core"]).toBe("^2.0.0");
    expect((await Bun.file(join(root, "packages/independent/package.json")).json()).version).toBe(
      "7.0.0",
    );
    expect((await Bun.file(join(root, "packages/hidden/package.json")).json()).version).toBe(
      "1.0.0",
    );
    expect((await Bun.file(join(root, "examples/demo/package.json")).json()).version).toBe("1.0.0");
    for (const directory of [
      "packages/hidden",
      "examples/demo",
      "templates/demo",
      "packages/core/examples/plugin",
    ]) {
      expect(await Bun.file(join(root, directory, "CHANGELOG.md")).exists()).toBe(false);
    }
    expect(await Bun.file(join(root, "packages/core/CHANGELOG.md")).text()).toContain(
      "Change the fixture core contract.",
    );
    expect(await Bun.file(join(root, ".changeset/core-major.md")).exists()).toBe(false);

    await run(root, [process.execPath, "install", "--ignore-scripts"]);
    await run(root, [process.execPath, "install", "--frozen-lockfile", "--ignore-scripts"]);
    expect(await Bun.file(join(root, "bun.lock")).text()).toContain('"version": "2.0.0"');
    expect((await run(root, ["git", "rev-list", "--count", "HEAD"])).trim()).toBe("1");
    expect((await run(root, ["git", "tag", "--list"])).trim()).toBe("");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
