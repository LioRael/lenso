import { expect, test } from "bun:test";
import { cp, mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";

test("built root imports without optional runtime packages", async () => {
  // The resolver rejects optional packages even if ancestors have node_modules.
  const root = await mkdtemp(
    join(process.env.DELTA_SCRATCH_DIR ?? import.meta.dir, ".organization-root-"),
  );
  try {
    await cp(new URL("../dist", import.meta.url), join(root, "dist"), { recursive: true });
    const child = Bun.spawn(
      [
        process.execPath,
        "-e",
        `import { plugin } from "bun";
       plugin({ name: "deny-optional-packages", setup(build) {
         build.onResolve({filter: /^(@lenso\\/|drizzle-orm|@orpc\\/|@opentelemetry\\/)/},
           () => { throw new Error("root imported an optional package"); });
       }});
       const m = await import(${JSON.stringify(join(root, "dist/index.js"))});
       if (typeof m.createOrganizationService !== "function") throw new Error("missing service");
       if (m.resolveOrganizationConfig().maxMembers !== 1000) throw new Error("missing config");`,
      ],
      { cwd: root, stdout: "pipe", stderr: "pipe" },
    );
    const [exit, error] = await Promise.all([child.exited, new Response(child.stderr).text()]);
    expect(error).toBe("");
    expect(exit).toBe(0);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("public manifest isolates optional entries and includes reviewed migrations", async () => {
  const manifest = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
  expect(manifest.dependencies).toBeUndefined();
  for (const dependency of ["@lenso/auth", "@lenso/core", "drizzle-orm"]) {
    expect(manifest.peerDependenciesMeta[dependency].optional).toBe(true);
  }
  for (const entry of Object.values(manifest.exports)) {
    if (typeof entry === "object" && entry !== null && "types" in entry && "import" in entry) {
      for (const path of [entry.types, entry.import]) {
        expect(await Bun.file(new URL(`../${path}`, import.meta.url)).exists()).toBe(true);
      }
    }
  }
  for (const dialect of ["pg", "sqlite"]) {
    const sql = await readFile(
      new URL(`../migrations/${dialect}/0000_organizations.sql`, import.meta.url),
      "utf8",
    );
    expect(sql).toContain("CREATE TABLE organizations");
    expect(sql).not.toMatch(/users|email/i);
  }
});
