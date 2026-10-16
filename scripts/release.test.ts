import { expect, test } from "bun:test";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { prepareVersions } from "./release-version";
import {
  releaseOrder,
  validateArchive,
  type Manifest,
  type ReleasePackage,
} from "./release-verify";

const core: Manifest = {
  name: "@lenso/core",
  version: "0.1.0",
  exports: { ".": "./dist/index.js" },
};
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
        name: "@lenso/cli",
        version: "0.1.0",
        dependencies: { "@lenso/engine": "workspace:^" },
      }),
      pkg({ name: "app", version: "0.1.0", private: true }),
      pkg({
        name: "@lenso/engine",
        version: "0.1.0",
        dependencies: { "@lenso/core": "workspace:^" },
      }),
      pkg(core),
    ]).map((p) => p.manifest.name),
  ).toEqual(["@lenso/core", "@lenso/engine", "@lenso/cli"]);
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
  expect(() => releaseOrder([pkg({ ...core, dependencies: { "@lenso/core": "*" } })])).toThrow(
    "cycle",
  );
});

test("foreign staged SDK is build-only, never a publication candidate or unresolved runtime exception", () => {
  expect(
    releaseOrder([
      pkg({
        ...core,
        devDependencies: { "@lenso/runtime-sdk": "workspace:*" },
        peerDependencies: { "@lenso/runtime-sdk": "0.1.0" },
      }),
    ]).map((entry) => entry.manifest.name),
  ).toEqual(["@lenso/core"]);
  expect(() =>
    releaseOrder([pkg({ ...core, dependencies: { "@lenso/runtime-sdk": "workspace:*" } })]),
  ).toThrow("unresolved");
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
      expect(() =>
        validateArchive(core, { ...core, [group]: { "@lenso/core": range } }, entries),
      ).toThrow("still uses");
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
      dependencies: { "@lenso/core": "workspace:^" },
      peerDependencies: { "@lenso/core": "workspace:~" },
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
    expect(packed.dependencies["@lenso/core"]).toBe("^0.1.0");
    expect(packed.peerDependencies["@lenso/core"]).toBe("~0.1.0");
    const actualEntries = (await run(root, ["tar", "-tzf", archive])).trim().split("\n");
    validateArchive(manifest, packed, actualEntries);
    expect(actualEntries).toContain("package/dist/bin.js");
    expect(actualEntries).toContain("package/dist/index.d.ts");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test.each([false, true])(
  "Changesets versions a workspace with scoped preparation=%s",
  async (scoped) => {
    const root = await mkdtemp(join(tmpdir(), "lenso-changesets-test-"));
    const cli = join(import.meta.dir, "../node_modules/@changesets/cli/bin.js");
    const config = await Bun.file(join(import.meta.dir, "../.changeset/config.json")).json();
    const source = await Bun.file(join(import.meta.dir, "../package.json")).json();
    const runtime = await Bun.file(
      join(import.meta.dir, "../packages/runtime/package.json"),
    ).json();
    const example = await Bun.file(
      join(import.meta.dir, "../examples/runtime/package.json"),
    ).json();
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
        peerDependencies: { "fixture-core": scoped ? "^1.0.0 || ^2.0.0" : "^1.0.0" },
      },
      independent: { name: "fixture-independent", version: "7.0.0" },
      runtime: {
        name: "fixture-runtime",
        version: "0.1.0",
        devDependencies: { "@lenso/runtime-sdk": runtime.devDependencies["@lenso/runtime-sdk"] },
        peerDependencies: { "@lenso/runtime-sdk": runtime.peerDependencies["@lenso/runtime-sdk"] },
      },
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
        JSON.stringify({
          private: true,
          version: "1.0.0",
          workspaces: [
            "packages/*",
            "examples/*",
            ...source.workspaces.filter((path: string) => path === ".runtime-deps/sdk"),
          ],
          devDependencies: { "@lenso/runtime-sdk": source.devDependencies["@lenso/runtime-sdk"] },
        }),
      );
      await Bun.write(
        join(root, ".runtime-deps/sdk/package.json"),
        JSON.stringify({ name: "@lenso/runtime-sdk", version: "0.1.0" }),
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
          dependencies: {
            "fixture-core": "workspace:^",
            "@lenso/runtime-sdk": example.dependencies["@lenso/runtime-sdk"],
          },
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
      const pending = '---\n"fixture-independent": minor\n---\n\nUnrelated pending release.\n';
      await Bun.write(join(root, ".changeset/unrelated.md"), pending);
      await run(root, [process.execPath, "install", "--ignore-scripts"]);
      await run(root, [process.execPath, cli, "add", "--empty"]);
      await expect(prepareVersions(root, { changesets: ["missing"], write: true })).rejects.toThrow(
        "Unknown pending",
      );
      const plan = await prepareVersions(root, { changesets: ["core-major"] });
      expect(plan.releases.some((release) => release.name === "@lenso/runtime-sdk")).toBe(false);
      expect(
        plan.releases
          .filter((release: { type: string }) => release.type !== "none")
          .map((release: { name: string }) => release.name)
          .sort(),
      ).toEqual(
        scoped
          ? ["fixture-consumer", "fixture-core"]
          : ["fixture-consumer", "fixture-core", "fixture-peer"],
      );
      for (const name of ["fixture-hidden", "fixture-example"]) {
        expect(
          plan.releases.find((release: { name: string }) => release.name === name),
        ).toMatchObject({ type: "none", newVersion: "1.0.0" });
      }
      expect((await Bun.file(join(root, "packages/core/package.json")).json()).version).toBe(
        "1.0.0",
      );

      const scope = scoped ? { packages: ["fixture-core"] } : {};
      if (scoped) {
        for (const names of [
          [],
          ["fixture-core", "fixture-core"],
          ["fixture-peer"],
          ["fixture-hidden"],
          ["unknown"],
        ]) {
          await expect(
            prepareVersions(root, { changesets: ["core-major"], packages: names, write: true }),
          ).rejects.toThrow();
        }
        await expect(
          prepareVersions(root, { packages: ["fixture-core"], write: true }),
        ).rejects.toThrow("explicit changesets");
        const preview = await prepareVersions(root, { changesets: ["core-major"], ...scope });
        expect(preview.releases.map((r) => r.name)).toEqual(["fixture-core"]);
        expect(preview.deferred.map((r) => r.name).sort()).toEqual(["fixture-consumer"]);
        expect(
          await Bun.file(join(root, `.changeset/${preview.deferredChangeset}.md`)).exists(),
        ).toBe(false);
      }
      const applied = await prepareVersions(root, {
        changesets: ["core-major"],
        ...scope,
        write: true,
      });
      if (scoped) {
        const deferred = await Bun.file(
          join(root, `.changeset/${applied.deferredChangeset}.md`),
        ).text();
        expect(deferred).toContain('"fixture-consumer": patch');
        expect(deferred).not.toContain('"fixture-peer": patch');
        expect(await Bun.file(join(root, "packages/consumer/CHANGELOG.md")).exists()).toBe(false);
        // Deferred intent remains executable by the ordinary, unscoped Changesets path.
        const later = await prepareVersions(root);
        expect(later.releases.find((r) => r.name === "fixture-consumer")?.newVersion).toBe("1.0.1");
        expect(later.releases.find((r) => r.name === "fixture-peer")).toBeUndefined();
      }
      const coreManifest = await Bun.file(join(root, "packages/core/package.json")).json();
      const consumer = await Bun.file(join(root, "packages/consumer/package.json")).json();
      const peer = await Bun.file(join(root, "packages/peer/package.json")).json();
      expect(coreManifest.version).toBe("2.0.0");
      expect(consumer.version).toBe(scoped ? "1.0.0" : "1.0.1");
      expect(consumer.dependencies["fixture-core"]).toBe("workspace:^");
      expect(peer.peerDependencies["fixture-core"]).toBe(scoped ? "^1.0.0 || ^2.0.0" : "^2.0.0");
      expect((await Bun.file(join(root, "packages/independent/package.json")).json()).version).toBe(
        "7.0.0",
      );
      expect((await Bun.file(join(root, "packages/hidden/package.json")).json()).version).toBe(
        "1.0.0",
      );
      expect((await Bun.file(join(root, "examples/demo/package.json")).json()).version).toBe(
        "1.0.0",
      );
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
      expect(await Bun.file(join(root, ".changeset/unrelated.md")).text()).toBe(pending);
      expect(await Bun.file(join(root, ".runtime-deps/sdk/package.json")).json()).toEqual({
        name: "@lenso/runtime-sdk",
        version: "0.1.0",
      });

      await run(root, [process.execPath, "install", "--ignore-scripts"]);
      await run(root, [process.execPath, "install", "--frozen-lockfile", "--ignore-scripts"]);
      const lock = await Bun.file(join(root, "bun.lock")).text();
      await rm(join(root, "node_modules"), { recursive: true, force: true });
      await run(root, [process.execPath, "install", "--frozen-lockfile", "--ignore-scripts"]);
      expect(await Bun.file(join(root, "bun.lock")).text()).toBe(lock);
      await Bun.write(
        join(root, ".changeset/foreign-sdk.md"),
        '---\n"@lenso/runtime-sdk": patch\n---\n\nForeign SDK cannot be versioned here.\n',
      );
      await expect(
        prepareVersions(root, { changesets: ["foreign-sdk"], write: true }),
      ).rejects.toThrow("foreign package");
      expect(await Bun.file(join(root, "bun.lock")).text()).toContain('"version": "2.0.0"');
      expect((await run(root, ["git", "rev-list", "--count", "HEAD"])).trim()).toBe("1");
      expect((await run(root, ["git", "tag", "--list"])).trim()).toBe("");
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  },
);
