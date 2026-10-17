import { expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";

test("published entries isolate the Bun-only SQLite store from portable consumers", async () => {
  const root = await mkdtemp(join(import.meta.dir, ".entries-"));
  try {
    const portable = join(root, "portable.ts");
    const sqlite = join(root, "sqlite.ts");
    await writeFile(
      portable,
      `export { createEvents } from "@lenso/events";
export { createEventsPlugin } from "@lenso/events/plugin";
export { createAsyncEvents } from "@lenso/events/async";
export { createAsyncEventsPlugin } from "@lenso/events/async-plugin";`,
    );
    await writeFile(sqlite, `export { createSqliteEventStore } from "@lenso/events/sqlite";`);
    for (const target of ["browser", "node"] as const) {
      const result = await Bun.build({ entrypoints: [portable], target, packages: "bundle" });
      expect(result.success).toBe(true);
      for (const output of result.outputs) {
        const code = await output.text();
        expect(code).not.toContain("bun:sqlite");
        expect(code).not.toContain("node:fs");
      }
      const blocked = await Bun.build({
        entrypoints: [sqlite],
        target,
        packages: "bundle",
        throw: false,
      });
      expect(blocked.success).toBe(false);
      expect(blocked.logs.some((log) => log.message.includes("@lenso/events/sqlite"))).toBe(true);
    }
    const native = await Bun.build({ entrypoints: [sqlite], target: "bun", packages: "bundle" });
    expect(native.success).toBe(true);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
