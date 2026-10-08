import { expect, test } from "bun:test";
import { cp, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

test("built root imports and runs without installing any optional integration", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "lenso-limits-root-"));
  try {
    const root = fileURLToPath(new URL("../../..", import.meta.url));
    const core = join(fixture, "node_modules/@lenso/core");
    await mkdir(core, { recursive: true });
    await cp(join(root, "packages/lenso/dist"), join(core, "dist"), { recursive: true });
    await cp(join(root, "packages/lenso/package.json"), join(core, "package.json"));
    await cp(join(root, "packages/limits/dist"), join(fixture, "dist"), { recursive: true });
    await writeFile(
      join(fixture, "run.ts"),
      `
      import { createLimits, createMemoryLimitStore } from "./dist/index.js";
      const service = createLimits({store:createMemoryLimitStore(),config:{failurePolicy:"throw"}});
      const result = await service.consumeRate({
        scope:{instance:"root",tenant:"fixture",key:"no-optional-deps"},
        capacity:1,quantity:1,periodMs:1000,
      });
      if (!result.allowed || result.remaining !== 0) throw new Error("root entry failed");
      await service.close();
    `,
    );
    const child = Bun.spawn([process.execPath, join(fixture, "run.ts")], {
      stdout: "pipe",
      stderr: "pipe",
    });
    const [code, stderr] = await Promise.all([child.exited, new Response(child.stderr).text()]);
    expect(stderr).toBe("");
    expect(code).toBe(0);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});
