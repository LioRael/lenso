import { expect, test } from "bun:test";
import { mkdtemp, realpath, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { defineSource } from "@lenso/auth";
import { withApp } from "@lenso/core";
import { createNotesApp } from "../app";

test("real Auth/Storage/Cache composition preserves authorization on cache hits and closes on failure", async () => {
  const root = await mkdtemp(join(await realpath(tmpdir()), "lenso-assembly-test-"));
  const proof = crypto.randomUUID();
  let active = true;
  let verifications = 0;
  const source = defineSource({
    async verify(evidence: string) {
      verifications++;
      return active && evidence === proof
        ? { status: "verified" as const, subjectId: "alice" }
        : { status: "rejected" as const };
    },
  });
  try {
    const { app, notes, cache } = createNotesApp(root, source);
    expect(app.plugins.map((plugin) => plugin.id)).toEqual([
      "auth",
      "storage",
      "memory",
      "note-cache",
      "notes",
    ]);
    const failure = new Error("application failed");
    let closedCache: (() => Promise<unknown>) | undefined;
    await expect(
      withApp(app, async (running) => {
        const service = running.get(notes);
        const cacheService = running.get(cache);
        closedCache = () => cacheService.get("anything");
        await service.put(proof, "alice", "private note");
        const text: string = await service.read(proof, "alice");
        expect(text).toBe("private note");
        expect(await cacheService.get("notes/alice.txt")).toMatchObject({
          status: "hit",
          value: text,
        });
        expect(await service.read(proof, "alice")).toBe(text);
        expect(verifications).toBe(3);
        await expect(service.read(proof, "bob")).rejects.toMatchObject({ code: "FORBIDDEN" });
        active = false;
        await expect(service.read(proof, "alice")).rejects.toMatchObject({ code: "UNAUTHORIZED" });
        throw failure;
      }),
    ).rejects.toBe(failure);
    await expect(closedCache!()).rejects.toMatchObject({ code: "closed" });
    active = true;
    const retry = createNotesApp(root, source);
    expect(
      await withApp(retry.app, (running) => running.get(retry.notes).read(proof, "alice")),
    ).toBe("private note");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
