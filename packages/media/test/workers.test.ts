import { expect, test } from "bun:test";

test("Workers control-plane bundle has no native processor, filesystem or Bun SQLite dependency", async () => {
  const result = await Bun.build({
    entrypoints: [new URL("./worker-entry.ts", import.meta.url).pathname],
    target: "browser",
    external: ["node:*"],
    format: "esm",
  });
  expect(result.success).toBe(true);
  const bundle = await result.outputs[0]!.text();
  for (const forbidden of [
    "sharp-child",
    "sharp/lib",
    'from "sharp"',
    'import("sharp")',
    "Bun.spawn",
    "bun:sqlite",
    "node:fs",
  ]) {
    expect(bundle.includes(forbidden)).toBe(false);
  }
  expect(bundle).toContain("createMedia");
  expect(bundle).toContain("createD1MediaStore");
});
