import { afterEach, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { cp, mkdir, mkdtemp, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { prepareRuntime, type ArchiveFetcher } from "./prepare-runtime";
import snapshot from "./runtime-sdk.json";

const roots: string[] = [];
const prefix = `lenso-runtime-${snapshot.commit}/`;
const sdkManifest = {
  name: "@lenso/runtime-sdk",
  version: "0.1.0",
  type: "module",
  files: ["dist"],
  exports: { ".": "./dist/index.js" },
  scripts: { build: "fixture" },
};
const archive = Bun.gzipSync(
  await new Bun.Archive({
    [`${prefix}package.json`]: JSON.stringify({ private: true }),
    [`${prefix}bun.lock`]: "{}",
    [`${prefix}sdk/typescript/package.json`]: JSON.stringify(sdkManifest),
    [`${prefix}tools/generate.ts`]: "// pinned generator fixture\n",
    [`${prefix}examples/contracts/logs.json`]: "{}",
  }).bytes(),
);
const pin = { ...snapshot, hash: createHash("sha256").update(archive).digest("hex") };
const fetchFixture = (async (url, options) => {
  expect(String(url)).toBe(
    `https://codeload.github.com/LioRael/lenso-runtime/tar.gz/${pin.commit}`,
  );
  expect(options?.signal).toBeInstanceOf(AbortSignal);
  return new Response(archive);
}) as ArchiveFetcher;

afterEach(async () => {
  for (const root of roots.splice(0)) await rm(root, { recursive: true, force: true });
});

async function fixture() {
  const scratch = process.env.DELTA_SCRATCH_DIR ?? process.env.TMPDIR ?? process.env.TEMP ?? "/tmp";
  const root = await mkdtemp(join(scratch, "runtime-prep-test-"));
  roots.push(root);
  await writeFile(
    join(root, "package.json"),
    JSON.stringify({ name: "lenso-workspace", private: true, engines: { bun: "1.4.4" } }),
  );
  return root;
}

async function buildFixture(args: string[], cwd: string) {
  if (args[0] !== "build") return;
  const dist = join(cwd, "dist");
  await mkdir(dist, { recursive: true });
  for (const file of ["index.js", "index.d.ts", "schema.js", "schema.d.ts"]) {
    await writeFile(join(dist, file), `// built fixture ${file}\n`);
  }
}

// These cases protect the preparation transaction and command boundary; adapter tests
// cannot observe checksum gating, pre-install bootstrap, or prior-artifact preservation.
test("checksum mismatch rejects before extraction, commands, or ready-state writes", async () => {
  const root = await fixture();
  let commands = 0;
  await expect(
    prepareRuntime(root, {
      fetch: (async () => new Response("wrong archive")) as ArchiveFetcher,
      run: async () => {
        commands++;
      },
    }),
  ).rejects.toThrow("checksum mismatch");
  expect(commands).toBe(0);
  expect(await readdir(root)).toEqual(["package.json"]);
});

test("only frozen script-free install and SDK-only build execute; repeated prep rebuilds source and provenance", async () => {
  const root = await fixture();
  const commands: string[][] = [];
  const run = async (args: string[], cwd: string) => {
    expect(cwd.startsWith(join(root, ".runtime-deps/.prepare-"))).toBe(true);
    commands.push(
      args.map((arg) =>
        arg.includes("/source/node_modules/")
          ? "<source>/node_modules/" + arg.split("/source/node_modules/")[1]
          : arg,
      ),
    );
    await buildFixture(args, cwd);
  };
  await prepareRuntime(root, { fetch: fetchFixture, run, pin });
  const output = join(root, ".runtime-deps");
  const provenance = await readFile(join(output, "provenance.json"), "utf8");
  expect(JSON.parse(provenance)).toMatchObject({ ...pin, bun: "1.4.4" });
  expect((await readdir(join(output, "sdk"))).sort()).toEqual(["dist", "package.json"]);
  expect(
    JSON.parse(await readFile(join(output, "sdk/package.json"), "utf8")).scripts,
  ).toBeUndefined();
  expect(await readFile(join(output, "source/tools/generate.ts"), "utf8")).toContain(
    "pinned generator",
  );
  await writeFile(join(output, "sdk/dist/index.js"), "tampered cache");
  await prepareRuntime(root, { fetch: fetchFixture, run, pin });
  expect(await readFile(join(output, "provenance.json"), "utf8")).toBe(provenance);
  expect(await readFile(join(output, "sdk/dist/index.js"), "utf8")).not.toContain("tampered");
  expect(commands).toEqual([
    ["install", "--frozen-lockfile", "--ignore-scripts"],
    [
      "build",
      "index.ts",
      "schema.ts",
      "--outdir",
      "dist",
      "--target",
      "bun",
      "--packages",
      "external",
      "--splitting",
    ],
    ["<source>/node_modules/typescript/bin/tsc", "-p", "tsconfig.build.json"],
    ["install", "--frozen-lockfile", "--ignore-scripts"],
    [
      "build",
      "index.ts",
      "schema.ts",
      "--outdir",
      "dist",
      "--target",
      "bun",
      "--packages",
      "external",
      "--splitting",
    ],
    ["<source>/node_modules/typescript/bin/tsc", "-p", "tsconfig.build.json"],
  ]);
  expect((await readdir(output)).sort()).toEqual([".owner", "provenance.json", "sdk", "source"]);
});

test("failed SDK build preserves all previous ready state", async () => {
  const root = await fixture();
  await prepareRuntime(root, { fetch: fetchFixture, run: buildFixture, pin });
  const ready = join(root, ".runtime-deps");
  const before = await readFile(join(ready, "provenance.json"), "utf8");
  await expect(
    prepareRuntime(root, {
      fetch: fetchFixture,
      pin,
      run: async (args) => {
        if (args[0] === "build") throw new Error("build failed");
      },
    }),
  ).rejects.toThrow("build failed");
  expect(await readFile(join(ready, "provenance.json"), "utf8")).toBe(before);
  expect(await readFile(join(ready, "sdk/dist/index.js"), "utf8")).toContain("built fixture");
  expect(await readFile(join(ready, "source/tools/generate.ts"), "utf8")).toContain(
    "pinned generator",
  );
  expect((await readdir(ready)).sort()).toEqual([".owner", "provenance.json", "sdk", "source"]);
});

test("cancelled SDK preparation releases its lock and preserves the ready artifact", async () => {
  const root = await fixture();
  await prepareRuntime(root, { fetch: fetchFixture, run: buildFixture, pin });
  const output = join(root, ".runtime-deps");
  const before = await readFile(join(output, "provenance.json"), "utf8");
  const abort = new AbortController();
  const reason = new Error("workspace cancelled");
  await expect(
    prepareRuntime(
      root,
      {
        fetch: fetchFixture,
        pin,
        async run(args, _cwd, signal) {
          expect(signal).toBe(abort.signal);
          if (args[0] === "build") {
            abort.abort(reason);
            signal!.throwIfAborted();
          }
        },
      },
      { signal: abort.signal },
    ),
  ).rejects.toBe(reason);
  expect(await readFile(join(output, "provenance.json"), "utf8")).toBe(before);
  expect((await readdir(output)).sort()).toEqual([".owner", "provenance.json", "sdk", "source"]);
});

test("explicit local source captures dirty tracked bytes and rejects symlinks without replacing ready state", async () => {
  const root = await fixture();
  const runtime = await fixture();
  await writeFile(
    join(runtime, "package.json"),
    JSON.stringify({ name: "lenso-runtime-workspace", private: true }),
  );
  await mkdir(join(runtime, "sdk/typescript"), { recursive: true });
  await writeFile(join(runtime, "sdk/typescript/package.json"), JSON.stringify(sdkManifest));
  await writeFile(join(runtime, "sdk/typescript/index.ts"), "first");
  for (const args of [
    ["init", "-q"],
    ["add", "package.json", "sdk"],
  ]) {
    expect(await Bun.spawn(["git", ...args], { cwd: runtime }).exited).toBe(0);
  }
  await writeFile(join(runtime, ".env"), "secret");
  await writeFile(join(runtime, "sdk/typescript/untracked.ts"), "untracked");
  await mkdir(join(runtime, "crates/sdk/tests"), { recursive: true });
  await writeFile(join(runtime, "crates/sdk/tests/schema-cases.json"), "[]");
  const run = async (args: string[], cwd: string) => {
    await buildFixture(args, cwd);
    if (args[0] === "build")
      await writeFile(join(cwd, "dist/index.js"), await readFile(join(cwd, "index.ts")));
  };
  const prepare = () =>
    prepareRuntime(
      root,
      {
        run,
        fetch: async () => {
          throw new Error("unexpected download");
        },
      },
      { runtimeRoot: runtime },
    );
  await prepare();
  const output = join(root, ".runtime-deps");
  const before = JSON.parse(await readFile(join(output, "provenance.json"), "utf8"));
  expect(before.source.kind).toBe("local");
  expect(before.commit).toBeUndefined();
  expect(before.source.files[".env"]).toBeUndefined();
  expect(before.source.files["sdk/typescript/untracked.ts"]).toBeUndefined();
  expect(before.source.files["crates/sdk/tests/schema-cases.json"]).toBeDefined();
  await writeFile(join(runtime, "sdk/typescript/index.ts"), "dirty current");
  await prepare();
  const after = JSON.parse(await readFile(join(output, "provenance.json"), "utf8"));
  expect(after.source.sha256).not.toBe(before.source.sha256);
  expect(after.files["dist/index.js"]).not.toBe(before.files["dist/index.js"]);
  expect(await readFile(join(output, "sdk/dist/index.js"), "utf8")).toBe("dirty current");
  await rm(join(runtime, "sdk/typescript/index.ts"));
  await symlink(join(runtime, ".env"), join(runtime, "sdk/typescript/index.ts"));
  await expect(prepare()).rejects.toThrow("regular files");
  expect(JSON.parse(await readFile(join(output, "provenance.json"), "utf8"))).toEqual(after);
});

test("unowned output and non-consuming roots are never replaced", async () => {
  const root = await fixture();
  await mkdir(join(root, ".runtime-deps"));
  await writeFile(join(root, ".runtime-deps/user.txt"), "keep");
  await expect(prepareRuntime(root, { fetch: fetchFixture, pin })).rejects.toThrow("unowned");
  expect(await readFile(join(root, ".runtime-deps/user.txt"), "utf8")).toBe("keep");
  await writeFile(join(root, "package.json"), JSON.stringify({ name: "other", private: true }));
  await expect(prepareRuntime(root, { fetch: fetchFixture, pin })).rejects.toThrow("consuming");
});

test("oversized download rejects before commands and output creation", async () => {
  const root = await fixture();
  await expect(
    prepareRuntime(root, {
      fetch: (async () =>
        new Response("small", {
          headers: { "content-length": String(21 * 1024 * 1024) },
        })) as ArchiveFetcher,
    }),
  ).rejects.toThrow("byte limit");
  await expect(
    prepareRuntime(root, {
      fetch: (async () =>
        new Response(
          new ReadableStream({
            start(controller) {
              for (let i = 0; i < 21; i++) controller.enqueue(new Uint8Array(1024 * 1024));
              controller.close();
            },
          }),
        )) as ArchiveFetcher,
    }),
  ).rejects.toThrow("byte limit");
  expect(await readdir(root)).toEqual(["package.json"]);
});

test("bootstrap runs from a clean consuming fixture using only Bun and standard library imports", async () => {
  const root = await fixture();
  await mkdir(join(root, "scripts"));
  await cp(join(import.meta.dir, "prepare-runtime.ts"), join(root, "scripts/prepare-runtime.ts"));
  await cp(join(import.meta.dir, "runtime-sdk.json"), join(root, "scripts/runtime-sdk.json"));
  await writeFile(join(root, "fixture.tar.gz"), archive);
  await writeFile(
    join(root, "bootstrap.ts"),
    `
    import { prepareRuntime } from "./scripts/prepare-runtime";
    import { mkdir, writeFile } from "node:fs/promises";
    import { join } from "node:path";
    await prepareRuntime(import.meta.dir, {
      pin: ${JSON.stringify(pin)},
      fetch: async () => new Response(await Bun.file(join(import.meta.dir, "fixture.tar.gz")).bytes()),
      run: async (args, cwd) => {
        if (args[0] === "install") return;
        const dist = join(cwd, "dist");
        await mkdir(dist, { recursive: true });
        for (const name of ["index.js", "index.d.ts", "schema.js", "schema.d.ts"]) {
          await writeFile(join(dist, name), "// clean bootstrap");
        }
      },
    });
  `,
  );
  const child = Bun.spawn([process.execPath, join(root, "bootstrap.ts")], {
    cwd: root,
    stdout: "pipe",
    stderr: "pipe",
  });
  const error = await new Response(child.stderr).text();
  expect(await child.exited, error).toBe(0);
  expect(await readdir(root)).not.toContain("node_modules");
  expect(await readFile(join(root, ".runtime-deps/sdk/dist/index.js"), "utf8")).toContain(
    "clean bootstrap",
  );
});

test("Runtime compiler entry executes with Bun when PATH contains no Node", async () => {
  const root = await fixture();
  const compiler = join(root, "compiler.js");
  await writeFile(
    compiler,
    "#!/usr/bin/env node\nconsole.log(`compiler-runtime:${process.versions.bun}`);\n",
  );
  const wrapper = join(root, "compiler-check.ts");
  await writeFile(
    wrapper,
    `
    import { runRuntimeCommand } from ${JSON.stringify(join(import.meta.dir, "prepare-runtime.ts"))};
    await runRuntimeCommand([${JSON.stringify(compiler)}], import.meta.dir);
  `,
  );
  const child = Bun.spawn([process.execPath, wrapper], {
    cwd: root,
    env: { PATH: "", HOME: root, TMPDIR: root },
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  expect(code, stderr).toBe(0);
  expect(stdout).toContain(`compiler-runtime:${Bun.version}`);
});

test("Runtime command retains underlying stdout/stderr and propagates its nonzero exit", async () => {
  const root = await fixture();
  const wrapper = join(root, "failure-check.ts");
  await writeFile(
    wrapper,
    `
    import { runRuntimeCommand } from ${JSON.stringify(join(import.meta.dir, "prepare-runtime.ts"))};
    await runRuntimeCommand(["-e", "console.log('fixture-build-output');console.error('fixture-compiler-diagnostic');process.exit(23)"], import.meta.dir);
  `,
  );
  const child = Bun.spawn([process.execPath, wrapper], {
    cwd: root,
    env: { PATH: "", HOME: root, TMPDIR: root },
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  expect(code).not.toBe(0);
  expect(stdout).toContain("fixture-build-output");
  expect(stderr).toContain("fixture-compiler-diagnostic");
  expect(stderr).toContain("exit 23");
});
