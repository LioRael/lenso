import { expect, test } from "bun:test";

test("ordinary root and memory bundle without optional framework or native imports", async () => {
  const result = await Bun.build({
    entrypoints: [
      new URL("../src/index.ts", import.meta.url).pathname,
      new URL("../src/memory.ts", import.meta.url).pathname,
    ],
    target: "browser",
    packages: "bundle",
  });
  expect(result.success).toBe(true);
  expect(result.logs).toEqual([]);
  for (const output of result.outputs) {
    const text = await output.text();
    expect(text).not.toContain("@lenso/core");
    expect(text).not.toContain('from "bun"');
    expect(text).not.toContain("RedisClient");
    expect(text).not.toContain("Buffer.");
  }
});
