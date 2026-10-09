import { beforeAll, describe, expect, test } from "bun:test";
import { mkdtemp, readdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import sharp from "sharp";
import { createBunImageProcessor } from "../src/bun";
import {
  ProcessorError,
  type ImageFormat,
  type ProcessorErrorCode,
  type Recipe,
} from "../src/processor";
import { crc32, fixture, pngChunk } from "./native-fixtures";

const signal = () => new AbortController().signal;
const processor = createBunImageProcessor();
const recipe = (overrides: Partial<Recipe> = {}): Recipe => ({
  width: 20,
  height: 20,
  fit: "cover",
  format: "png",
  quality: 85,
  metadata: "strip",
  animation: "reject",
  ...overrides,
});
let png: Buffer;
beforeAll(async () => {
  png = await fixture();
});

async function rejectsCode(promise: Promise<unknown>, code: ProcessorErrorCode) {
  try {
    await promise;
    throw new Error(`Expected ${code}`);
  } catch (error) {
    expect(error).toBeInstanceOf(ProcessorError);
    expect((error as ProcessorError).code).toBe(code);
    expect((error as Error).message).toBe(`Image processing failed: ${code}`);
  }
}

describe("Bun sharp processor (real native decoding)", () => {
  test("metadata describes encoded dimensions and transparency", async () => {
    expect(await processor.inspect(png, signal())).toEqual({
      format: "png",
      mime: "image/png",
      width: 80,
      height: 40,
      orientation: 1,
      hasAlpha: true,
      frames: 1,
    });
  });

  test("EXIF orientation is reported, applied, and stripped with GPS", async () => {
    const jpeg = await sharp(png)
      .jpeg()
      .withMetadata({ orientation: 6 })
      .withExifMerge({
        IFD0: { Artist: "fixture" },
        IFD3: { GPSLatitudeRef: "N", GPSLatitude: "51/1 30/1 0/1" },
      })
      .toBuffer();
    expect((await processor.inspect(jpeg, signal())).orientation).toBe(6);
    const result = await processor.transform(
      jpeg,
      recipe({ width: 80, height: 80, fit: "inside", format: "jpeg" }),
      signal(),
    );
    expect(result.metadata).toMatchObject({ width: 40, height: 80, orientation: 1 });
    const decoded = await sharp(result.bytes).metadata();
    expect(decoded.exif).toBeUndefined();
    expect(decoded.orientation).toBeUndefined();
    expect(decoded.icc).toBeUndefined();
    expect(decoded.space).toBe("srgb");
  });

  for (const format of ["jpeg", "png", "webp"] as ImageFormat[]) {
    test(`${format}: signature, MIME, alpha and real pixels`, async () => {
      const result = await processor.transform(
        png,
        recipe({ format, width: 80, height: 40 }),
        signal(),
      );
      expect(result.metadata).toMatchObject({
        format,
        mime: `image/${format}`,
        hasAlpha: format !== "jpeg",
      });
      expect((await processor.inspect(result.bytes, signal())).format).toBe(format);
      if (format === "jpeg") expect([...result.bytes.subarray(0, 3)]).toEqual([255, 216, 255]);
      if (format === "png")
        expect([...result.bytes.subarray(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);
      if (format === "webp") {
        expect(Buffer.from(result.bytes.subarray(0, 4)).toString()).toBe("RIFF");
        expect(Buffer.from(result.bytes.subarray(8, 12)).toString()).toBe("WEBP");
      }
      const raw = await sharp(result.bytes).raw().toBuffer({ resolveWithObject: true });
      if (format === "jpeg") expect([...raw.data.subarray(0, 3)]).toEqual([255, 255, 255]);
      else expect(raw.data[3]).toBe(0);
    });
  }

  test("cover crops, inside preserves aspect, contain pads transparently", async () => {
    for (const fit of ["cover", "inside", "contain"] as const) {
      const result = await processor.transform(png, recipe({ fit }), signal());
      expect(result.metadata.width).toBe(20);
      expect(result.metadata.height).toBe(fit === "inside" ? 10 : 20);
      if (fit === "contain") {
        const raw = await sharp(result.bytes).raw().toBuffer();
        expect(raw[3]).toBe(0);
      }
    }
  });

  test("rejects SVG, GIF and unsupported signatures", async () => {
    for (const bytes of [
      Buffer.from("<svg/>"),
      Buffer.from("GIF89a"),
      Buffer.from("not an image"),
    ]) {
      await rejectsCode(processor.inspect(bytes, signal()), "unsupported-image");
    }
  });

  test("rejects corrupt and truncated JPEG/PNG/WebP rather than trusting headers", async () => {
    for (const format of ["jpeg", "png", "webp"] as const) {
      const bytes = await sharp(png).toFormat(format).toBuffer();
      await rejectsCode(
        processor.inspect(bytes.subarray(0, Math.floor(bytes.length * 0.65)), signal()),
        "invalid-image",
      );
    }
    const corrupt = Buffer.from(png);
    corrupt[45] ^= 0xff;
    await rejectsCode(processor.inspect(corrupt, signal()), "invalid-image");
  });

  test("rejects huge pixel header before raw decode", async () => {
    const huge = Buffer.from(png);
    huge.writeUInt32BE(100_000, 16);
    huge.writeUInt32BE(100_000, 20);
    huge.writeUInt32BE(crc32(huge.subarray(12, 29)), 29);
    await rejectsCode(processor.inspect(huge, signal()), "limit-exceeded");
  });

  test("bounds animated frames and explicitly rejects even permitted animations", async () => {
    const raw = Buffer.alloc(8 * 16 * 4, 128);
    raw.fill(255, 8 * 8 * 4);
    const animated = await sharp(raw, { raw: { width: 8, height: 16, channels: 4, pageHeight: 8 } })
      .webp({ loop: 0, delay: [100, 100] })
      .toBuffer();
    expect((await sharp(animated).metadata()).pages).toBe(2);
    await rejectsCode(processor.inspect(animated, signal()), "limit-exceeded");
    await rejectsCode(
      createBunImageProcessor({ limits: { maxFrames: 2 } }).inspect(animated, signal()),
      "unsupported-image",
    );
    const control = Buffer.alloc(8);
    control.writeUInt32BE(2);
    const apng = Buffer.concat([png.subarray(0, 33), pngChunk("acTL", control), png.subarray(33)]);
    await rejectsCode(processor.inspect(apng, signal()), "limit-exceeded");
    control.writeUInt32BE(1);
    const singleFrameApng = Buffer.concat([
      png.subarray(0, 33),
      pngChunk("acTL", control),
      png.subarray(33),
    ]);
    await rejectsCode(processor.inspect(singleFrameApng, signal()), "unsupported-image");
    const chunks = [animated.subarray(0, 12)];
    let frameSeen = false;
    for (let offset = 12; offset < animated.length;) {
      const name = animated.toString("ascii", offset, offset + 4);
      const length = animated.readUInt32LE(offset + 4);
      const end = offset + 8 + length + (length % 2);
      if (name !== "ANMF" || !frameSeen) chunks.push(animated.subarray(offset, end));
      if (name === "ANMF") frameSeen = true;
      offset = end;
    }
    const singleFrameWebp = Buffer.concat(chunks);
    singleFrameWebp.writeUInt32LE(singleFrameWebp.length - 8, 4);
    expect((await sharp(singleFrameWebp).metadata()).pages).toBe(1);
    await rejectsCode(processor.inspect(singleFrameWebp, signal()), "unsupported-image");
  });

  test("bounds input, output, dimensions, pixels and memory", async () => {
    for (const limits of [
      { maxInputBytes: png.length - 1 },
      { maxWidth: 79 },
      { maxHeight: 39 },
      { maxPixels: 3199 },
      { maxMemoryBytes: 1 },
    ])
      await rejectsCode(
        createBunImageProcessor({ limits }).inspect(png, signal()),
        "limit-exceeded",
      );
    await rejectsCode(
      createBunImageProcessor({ limits: { maxOutputBytes: 10 } }).transform(
        png,
        recipe(),
        signal(),
      ),
      "limit-exceeded",
    );
    await rejectsCode(
      createBunImageProcessor({ limits: { maxMemoryBytes: 12 * 1024 * 1024 } }).inspect(
        png,
        signal(),
      ),
      "limit-exceeded",
    );
  });

  test("rejects unknown recipe fields, unsafe dimensions and uncontrolled policies", async () => {
    for (const invalid of [
      { width: NaN },
      { height: -1 },
      { width: 100_000 },
      { quality: 101 },
      { path: "/tmp/out" },
      { fit: "fill" },
      { metadata: "keep" },
      { animation: "all" },
    ])
      await rejectsCode(
        processor.transform(png, recipe(invalid as Partial<Recipe>), signal()),
        "limit-exceeded",
      );
    expect(() => createBunImageProcessor({ limits: { concurrency: 0 } })).toThrow(ProcessorError);
    expect(() => createBunImageProcessor({ maxQueue: -1 })).toThrow(ProcessorError);
  });

  test("deadline kills children; already-aborted and running cancellation are safe", async () => {
    await rejectsCode(
      createBunImageProcessor({ limits: { timeoutMs: 1 } }).inspect(png, signal()),
      "timeout",
    );
    const cancelled = new AbortController();
    cancelled.abort();
    await rejectsCode(processor.inspect(png, cancelled.signal), "cancelled");
    const running = new AbortController();
    const pending = processor.inspect(await fixture(1000, 1000), running.signal);
    setTimeout(() => running.abort(), 5);
    await rejectsCode(pending, "cancelled");
    expect((await processor.inspect(png, signal())).width).toBe(80);
  });

  test("bounded active jobs and waiting queue; queued cancellation frees capacity", async () => {
    const bounded = createBunImageProcessor({ limits: { concurrency: 1 }, maxQueue: 1 });
    const first = bounded.inspect(png, signal());
    const cancelled = new AbortController();
    const queued = bounded.inspect(png, cancelled.signal);
    await rejectsCode(bounded.inspect(png, signal()), "limit-exceeded");
    cancelled.abort();
    await rejectsCode(queued, "cancelled");
    const replacement = bounded.inspect(png, signal());
    expect((await first).width).toBe(80);
    expect((await replacement).width).toBe(80);
  });

  test("zero temporary disk budget succeeds without files and leaves no residue", async () => {
    const dirs = async () =>
      (await readdir(tmpdir())).filter((name) => name.startsWith("lenso-media-")).sort();
    const before = await dirs();
    await createBunImageProcessor({ limits: { maxTempBytes: 0 } }).transform(
      png,
      recipe(),
      signal(),
    );
    await rejectsCode(
      createBunImageProcessor({ limits: { timeoutMs: 1 } }).inspect(png, signal()),
      "timeout",
    );
    expect(await dirs()).toEqual(before);
  });

  test("compiled adjacent JS entry works; missing optional native dependency is unavailable", async () => {
    const directory = await mkdtemp(join(tmpdir(), "lenso-native-test-"));
    try {
      const build = await Bun.build({
        entrypoints: ["../src/bun.ts", "../src/processor.ts", "../src/sharp-child.ts"].map(
          (path) => new URL(path, import.meta.url).pathname,
        ),
        outdir: directory,
        target: "bun",
        format: "esm",
        external: ["sharp"],
      });
      expect(build.success).toBe(true);
      const compiled = await import(join(directory, "bun.js"));
      try {
        await compiled.createBunImageProcessor().inspect(png, signal());
        throw new Error("Expected missing sharp failure");
      } catch (error) {
        expect((error as ProcessorError).code).toBe("unavailable");
      }
      // Resolve the actual package-local installation rather than assuming root dependencies.
      const sharpRoot = dirname(dirname(import.meta.resolve("sharp").replace("file://", "")));
      await symlink(dirname(sharpRoot), join(directory, "node_modules"), "dir");
      expect((await compiled.createBunImageProcessor().inspect(png, signal())).width).toBe(80);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });
});
