import { expect, test } from "bun:test";
import { cp, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";

test("built root imports without any optional peers and exports remain isolated", async () => {
  const scratch = process.env.DELTA_SCRATCH_DIR ?? tmpdir();
  const directory = await mkdtemp(join(scratch, "api-key-package-"));
  try {
    await cp(new URL("../dist", import.meta.url), join(directory, "dist"), { recursive: true });
    const root = join(directory, "dist/index.js");
    const child = Bun.spawn(
      [
        process.execPath,
        "-e",
        `
      const api = await import(${JSON.stringify(root)});
      if (typeof api.createApiKeys !== "function") throw new Error("Missing service");
      if (api.createApiKeyPlugin || api.apiKeySource || api.createApiKeyManage) throw new Error("Optional entry leaked");
    `,
      ],
      {
        cwd: directory,
        env: {},
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const [exit, stderr] = await Promise.all([child.exited, new Response(child.stderr).text()]);
    expect(stderr).toBe("");
    expect(exit).toBe(0);
    const manifest = await Bun.file(new URL("../package.json", import.meta.url)).json();
    for (const [entry, target] of Object.entries(manifest.exports)) {
      if (entry.includes("*")) continue;
      const output = target as { types: string; default: string };
      expect(await Bun.file(new URL(`../${output.types}`, import.meta.url)).exists()).toBeTrue();
      expect(await Bun.file(new URL(`../${output.default}`, import.meta.url)).exists()).toBeTrue();
    }
    for (const peer of Object.keys(manifest.peerDependencies)) {
      expect(manifest.peerDependenciesMeta[peer].optional).toBeTrue();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
