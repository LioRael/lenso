import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

test("actual Bun CLI preload flushes finite command spans without losing JSON/exit status", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-telemetry-cli-"));
  try {
    for (const [command, code, fail] of [
      ["help", 0, false],
      ["unknown-private-input", 2, true],
    ] as const) {
      const output = join(directory, `trace-${code}.json`);
      const child = Bun.spawn(
        [
          process.execPath,
          "--preload",
          new URL("./fixtures/cli-preload.ts", import.meta.url).pathname,
          new URL("../../cli/dist/bin.js", import.meta.url).pathname,
          command,
          "--json",
        ],
        {
          env: {
            ...process.env,
            LENSO_TRACE_OUTPUT: output,
            ...(fail ? { LENSO_EXPORT_FAIL: "1" } : {}),
          },
          stdout: "pipe",
          stderr: "pipe",
        },
      );
      const [status, stdout, stderr] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      expect(status).toBe(code);
      expect(JSON.parse(stdout).ok).toBe(code === 0);
      const spans = await Bun.file(output).json();
      expect(spans[0].name).toBe("lenso.cli.command");
      expect(JSON.stringify(spans)).not.toContain("private-input");
      if (fail) expect(stderr).toContain("Telemetry flush failed");
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
