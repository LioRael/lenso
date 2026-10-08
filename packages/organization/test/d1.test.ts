import { test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

test("real local workerd/D1 organization backend contract", async () => {
  const root = await mkdtemp(join(process.env.DELTA_SCRATCH_DIR ?? tmpdir(), "organization-d1-"));
  try {
    const entry = join(root, "contract.mjs");
    const result = await Bun.build({
      entrypoints: [join(import.meta.dir, "d1-fixture.ts")],
      target: "node",
      naming: "contract.mjs",
      outdir: root,
      define: {
        "import.meta.url": JSON.stringify(new URL("./d1-fixture.ts", import.meta.url).href),
      },
    });
    if (!result.success) throw new AggregateError(result.logs, "D1 fixture build failed");
    // Miniflare's workerd bridge requires Node APIs that Bun does not implement.
    const child = Bun.spawn(["node", entry], {
      stdout: "pipe",
      stderr: "pipe",
      env: { ...process.env, NODE_NO_WARNINGS: "1" },
    });
    const timer = setTimeout(() => child.kill(), 25_000);
    try {
      const [exit, stdout, stderr] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      if (exit !== 0) throw new Error(`Node D1 contract failed (${exit}):\n${stdout}${stderr}`);
      console.info(stdout.trim());
    } finally {
      clearTimeout(timer);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}, 30_000);
