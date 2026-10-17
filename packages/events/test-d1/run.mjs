import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// Install tools separately so this fixture never changes the shared workspace lock.
const require = createRequire(
  process.env.LENSO_D1_TOOLING
    ? join(resolve(process.env.LENSO_D1_TOOLING), "package.json")
    : import.meta.url,
);
const { Miniflare } = require("miniflare");
const { build } = require("esbuild");
const entry = process.env.LENSO_D1_ENTRY
  ? resolve(process.env.LENSO_D1_ENTRY)
  : fileURLToPath(new URL("./worker.ts", import.meta.url));
const root = await mkdtemp(join(tmpdir(), "lenso-events-d1-"));
let runtime;
const started = performance.now();
try {
  const bindingsPath = join(root, "bindings.ts");
  await writeFile(
    bindingsPath,
    `
    import type { D1Database } from ${JSON.stringify(require.resolve("@cloudflare/workers-types/index.d.ts"))};
    import { createD1EventStore, initializeD1Events } from ${JSON.stringify(fileURLToPath(new URL("../src/d1.ts", import.meta.url)))};
    declare const db: D1Database;
    createD1EventStore(db);
    initializeD1Events(db);
    createD1EventStore(db.withSession("first-primary"));
  `,
  );
  const typecheck = spawnSync(
    process.execPath,
    [
      join(dirname(createRequire(import.meta.url).resolve("typescript/package.json")), "bin/tsc"),
      "--noEmit",
      "--strict",
      "--skipLibCheck",
      "--moduleResolution",
      "bundler",
      "--module",
      "esnext",
      "--target",
      "es2022",
      "--allowImportingTsExtensions",
      bindingsPath,
      entry,
    ],
    { encoding: "utf8" },
  );
  assert.equal(typecheck.status, 0, typecheck.stdout + typecheck.stderr);
  console.log("Official D1 binding/session types and acceptance fixture: passed");
  const output = await build({
    entryPoints: [entry],
    bundle: true,
    write: false,
    format: "esm",
    platform: "browser",
    target: "es2022",
  });
  const script = output.outputFiles[0].text;
  assert(!script.includes("bun:sqlite"), "D1 bundle must not import Bun SQLite");
  const options = {
    modules: true,
    script,
    compatibilityDate: "2026-07-30",
    d1Databases: { DB: "lenso-events-test" },
    d1Persist: join(root, "d1"),
  };
  runtime = new Miniflare(options);
  async function call(path, body) {
    const response = await runtime.dispatchFetch(
      `http://local.test${path}`,
      body
        ? {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify(body),
          }
        : {},
    );
    const result = await response.json();
    assert.equal(response.status, 200, JSON.stringify(result));
    return result;
  }
  console.log(JSON.stringify(await call("/checks"), null, 2));
  await call("/concurrent-seed");
  const claims = await Promise.all(Array.from({ length: 16 }, () => call("/concurrent-claim")));
  assert.equal(
    claims.filter(Boolean).length,
    1,
    "16 independent requests must have one claim winner",
  );
  console.log(JSON.stringify({ concurrentRequests: 16, claimWinners: 1 }));
  const oldClaim = await call("/seed");
  await runtime.dispose();
  runtime = new Miniflare(options);
  console.log(JSON.stringify(await call("/recover", oldClaim)));
  console.log(
    JSON.stringify({
      miniflare: require("miniflare/package.json").version,
      workerd: require("workerd/package.json").version,
      elapsedMs: Math.round(performance.now() - started),
      database: "temporary local D1, deleted after verification",
    }),
  );
} finally {
  await runtime?.dispose();
  await rm(root, { recursive: true, force: true });
}
