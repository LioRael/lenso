import { expect, test } from "bun:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { createR2StoragePlugin, type R2StorageBinding } from "../src/r2";

test("binding refuses unknown size and non-Workers uploads without touching the binding", async () => {
  const binding = {
    put() {
      throw new Error("must not call");
    },
  } as unknown as R2StorageBinding;
  const storage = await createR2StoragePlugin({ id: "r2", binding }).setup({
    onCleanup() {},
    get() {
      throw new Error("unused");
    },
  });
  await expect(
    storage.put({ key: "file", body: new Blob(["abc"]).stream() }),
  ).rejects.toMatchObject({ code: "invalid-input" });
  await expect(
    storage.put({ key: "file", body: new Blob(["abc"]).stream(), size: 3 }),
  ).rejects.toMatchObject({ code: "unsupported" });
  await expect(
    storage.signUpload({ key: "file", contentType: "text/plain", expiresIn: 60 }),
  ).rejects.toMatchObject({ code: "unsupported" });
});

test("native-shaped provider failures retain causes without exposing provider messages", async () => {
  for (const [status, code] of [
    [403, "forbidden"],
    [404, "not-found"],
    [412, "conflict"],
    [500, "provider"],
  ] as const) {
    const cause = Object.assign(new Error("private credential detail"), { status });
    const binding = {
      async get() {
        throw cause;
      },
    } as unknown as R2StorageBinding;
    const storage = await createR2StoragePlugin({ id: "errors", binding }).setup({
      onCleanup() {},
      get() {
        throw new Error("unused");
      },
    });
    await expect(storage.get("file")).rejects.toMatchObject({
      code,
      message: "R2 storage operation failed",
      cause,
    });
  }
});

test("native workerd R2 binding and FixedLengthStream: creates, conditions, limits, ranges, metadata, deletion", async () => {
  const built = await Bun.build({
    entrypoints: [new URL("../src/r2.ts", import.meta.url).pathname],
    target: "browser",
  });
  expect(built.success).toBe(true);
  const script = `${await built.outputs[0]!.text()}
    export default {
      async fetch(request, env) {
        const storage = await createR2StoragePlugin({ id: "r2", binding: env.BUCKET }).setup({
          onCleanup() {}, get() { throw new Error("unused"); }
        });
        const body = (text) => new Blob([text]).stream();
        const errors = {};
        const attempt = async (name, action) => { try { await action(); } catch (error) { errors[name] = error.code; } };
        const meta = await storage.put({ key: "folder/file", body: body("abcdef"), size: 6,
          contentType: "text/plain", customMetadata: { owner: "worker" } });
        await attempt("duplicate", () => storage.put({ key: "folder/file", body: body("changed"), size: 7 }));
        await attempt("large", () => storage.put({ key: "large", body: body("abcdef"), size: 6, maxBytes: 5 }));
        await attempt("short", () => storage.put({ key: "short", body: body("abc"), size: 4 }));
        await attempt("long", () => storage.put({ key: "long", body: body("abcde"), size: 4 }));
        await attempt("conditional", () => storage.get("folder/file", { ifMatch: '"other"' }));
        await attempt("missing", () => storage.get("missing"));
        const ranged = await storage.get("folder/file", { range: { offset: 1, length: 3 }, ifMatch: meta.etag });
        const text = await new Response(ranged.body).text();
        const fullText = await new Response((await storage.get("folder/file")).body).text();
        const head = await storage.head("folder/file");
        await storage.put({ key: "folder/second", body: body("second"), size: 6, contentType: "text/second" });
        const page = await storage.list({ prefix: "folder/", limit: 1 });
        const nextPage = await storage.list({ prefix: "folder/", limit: 1, cursor: page.cursor });
        const deleted = await storage.delete("folder/file");
        const absent = await storage.head("folder/file");
        const controller = new AbortController();
        let cancelled = false;
        const pending = storage.put({ key: "blocked", size: 1, signal: controller.signal,
          body: new ReadableStream({ pull: () => new Promise(() => {}), cancel() { cancelled = true; } }) });
        controller.abort();
        await attempt("abort", () => pending);
        return Response.json({ meta, errors, text, fullText, head, page, nextPage, deleted, absent,
          range: ranged.range, rangeSize: ranged.metadata.size, cancelled,
          invalidAbsent: [await storage.head("short"), await storage.head("long"), await storage.head("blocked")] });
      }
    };`;
  const mf = new Miniflare(
    convertV4MiniflareOptions({
      modules: true,
      script,
      compatibilityDate: "2026-10-08",
      r2Buckets: ["BUCKET"],
    }),
  );
  try {
    // Bun's fetch ignores Undici's dispatcher; use the actual local workerd listener.
    const response = await fetch(await mf.ready);
    expect(response.status).toBe(200);
    const result = (await response.json()) as {
      meta: { etag: string; versionId: string };
      errors: Record<string, string>;
      text: string;
      fullText: string;
      range: { offset: number; length: number };
      rangeSize: number;
      page: { objects: { contentType: string }[]; cursor?: string };
      nextPage: { objects: { contentType: string }[]; cursor?: string };
      deleted: { outcome: string };
      absent: unknown;
      invalidAbsent: unknown[];
      cancelled: boolean;
    };
    expect(result.meta).toMatchObject({
      size: 6,
      contentType: "text/plain",
      customMetadata: { owner: "worker" },
    });
    expect(result.meta.etag).toMatch(/^".+"$/);
    expect(result.meta.versionId).toBeString();
    expect(result.errors).toEqual({
      duplicate: "conflict",
      large: "too-large",
      short: "invalid-input",
      long: "invalid-input",
      conditional: "conflict",
      missing: "not-found",
      abort: "aborted",
    });
    expect(result.text).toBe("bcd");
    expect(result.fullText).toBe("abcdef");
    expect(result.range).toEqual({ offset: 1, length: 3 });
    expect(result.rangeSize).toBe(6);
    expect(result.page.objects[0].contentType).toBe("text/plain");
    expect(result.page.cursor).toBeString();
    expect(result.nextPage.objects[0].contentType).toBe("text/second");
    expect(result.nextPage.cursor).toBeUndefined();
    expect(result.deleted).toEqual({ outcome: "absent-or-deleted" });
    expect(result.absent).toBeNull();
    expect(result.invalidAbsent).toEqual([null, null, null]);
    expect(result.cancelled).toBe(true);
  } finally {
    await mf.dispose();
  }
}, 30000);
