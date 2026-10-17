import { expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";

test("published entries isolate Bun-only stores from portable and D1 consumers", async () => {
  const root = await mkdtemp(join(import.meta.dir, ".entries-"));
  try {
    const portable = join(root, "portable.ts");
    const sqlite = join(root, "sqlite.ts");
    const postgres = join(root, "postgres.ts");
    await writeFile(
      portable,
      `export { createEvents } from "@lenso/events";
export { createEventsPlugin } from "@lenso/events/plugin";
export { createAsyncEvents } from "@lenso/events/async";
export { createAsyncEventsPlugin } from "@lenso/events/async-plugin";
export { createD1EventStore } from "@lenso/events/d1";`,
    );
    await writeFile(sqlite, `export { createSqliteEventStore } from "@lenso/events/sqlite";`);
    await writeFile(postgres, `export { createPostgresEventStore } from "@lenso/events/postgres";`);
    for (const target of ["browser", "node"] as const) {
      const result = await Bun.build({ entrypoints: [portable], target, packages: "bundle" });
      expect(result.success).toBe(true);
      for (const output of result.outputs) {
        const code = await output.text();
        expect(code).not.toContain("bun:sqlite");
        expect(code).not.toContain("node:fs");
      }
      for (const [entry, path] of [
        [sqlite, "sqlite"],
        [postgres, "postgres"],
      ]) {
        const blocked = await Bun.build({
          entrypoints: [entry],
          target,
          packages: "bundle",
          throw: false,
        });
        expect(blocked.success).toBe(false);
        expect(blocked.logs.some((log) => log.message.includes(`@lenso/events/${path}`))).toBe(
          true,
        );
      }
    }
    const native = await Bun.build({
      entrypoints: [sqlite, postgres],
      target: "bun",
      packages: "bundle",
    });
    expect(native.success).toBe(true);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
