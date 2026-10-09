import {
  ProcessorError,
  type ImageFormat,
  type ImageMetadata,
  type ProcessorLimits,
  type Recipe,
} from "./processor";

const headerLimit = 4096;
const absoluteInputLimit = 256 * 1024 * 1024;
const crcTable = Array.from({ length: 256 }, (_, value) => {
  for (let bit = 0; bit < 8; bit++) value = (value >>> 1) ^ (value & 1 ? 0xedb88320 : 0);
  return value >>> 0;
});

function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = (crc >>> 8) ^ crcTable[(crc ^ byte) & 255]!;
  return (crc ^ 0xffffffff) >>> 0;
}

function signature(bytes: Uint8Array): ImageFormat {
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff)
    return "jpeg";
  if (bytes.length >= 8 && [137, 80, 78, 71, 13, 10, 26, 10].every((v, i) => bytes[i] === v))
    return "png";
  const text = (a: number, b: number) => new TextDecoder().decode(bytes.subarray(a, b));
  if (bytes.length >= 12 && text(0, 4) === "RIFF" && text(8, 12) === "WEBP") return "webp";
  throw new ProcessorError("unsupported-image");
}

function pngInfo(bytes: Uint8Array): { frames: number; animated: boolean } {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let frames = 1;
  let animated = false;
  for (let offset = 8; offset + 12 <= bytes.length;) {
    const length = view.getUint32(offset);
    if (length > bytes.length - offset - 12) throw new ProcessorError("invalid-image");
    if (
      crc32(bytes.subarray(offset + 4, offset + 8 + length)) !== view.getUint32(offset + 8 + length)
    ) {
      throw new ProcessorError("invalid-image");
    }
    if (
      bytes[offset + 4] === 97 &&
      bytes[offset + 5] === 99 &&
      bytes[offset + 6] === 84 &&
      bytes[offset + 7] === 76
    ) {
      if (length !== 8) throw new ProcessorError("invalid-image");
      animated = true;
      frames = view.getUint32(offset + 8);
      if (frames < 1) throw new ProcessorError("invalid-image");
    }
    if (
      bytes[offset + 4] === 73 &&
      bytes[offset + 5] === 69 &&
      bytes[offset + 6] === 78 &&
      bytes[offset + 7] === 68
    ) {
      if (length !== 0 || offset + 12 !== bytes.length) throw new ProcessorError("invalid-image");
      return { frames, animated };
    }
    offset += length + 12;
  }
  throw new ProcessorError("invalid-image");
}

function webpAnimated(bytes: Uint8Array): boolean {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let animated = false;
  let offset = 12;
  while (offset + 8 <= bytes.length) {
    const name = new TextDecoder().decode(bytes.subarray(offset, offset + 4));
    const length = view.getUint32(offset + 4, true);
    if (length > bytes.length - offset - 8) throw new ProcessorError("invalid-image");
    if (
      name === "ANIM" ||
      name === "ANMF" ||
      (name === "VP8X" && length > 0 && (bytes[offset + 8]! & 2) !== 0)
    )
      animated = true;
    offset += 8 + length + (length % 2);
  }
  if (offset !== bytes.length) throw new ProcessorError("invalid-image");
  return animated;
}

async function request(): Promise<{
  limits: ProcessorLimits;
  recipe?: Recipe;
  op: string;
  bytes: Uint8Array;
}> {
  const reader = Bun.stdin.stream().getReader();
  let header = new Uint8Array(0);
  let body: Uint8Array | undefined;
  let offset = 0;
  let parsed: { limits: ProcessorLimits; recipe?: Recipe; op: string; length: number } | undefined;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    let remaining = value;
    if (!parsed) {
      const newline = value.indexOf(10);
      const end = newline < 0 ? value.length : newline;
      if (header.length + end > headerLimit) throw new ProcessorError("unavailable");
      const combined = new Uint8Array(header.length + end);
      combined.set(header);
      combined.set(value.subarray(0, end), header.length);
      header = combined;
      if (newline < 0) continue;
      parsed = JSON.parse(new TextDecoder().decode(header));
      if (
        !parsed ||
        !Number.isSafeInteger(parsed.length) ||
        parsed.length < 1 ||
        parsed.length > absoluteInputLimit ||
        parsed.length > parsed.limits.maxInputBytes ||
        !["inspect", "transform"].includes(parsed.op)
      )
        throw new ProcessorError("limit-exceeded");
      body = new Uint8Array(parsed.length);
      remaining = value.subarray(newline + 1);
    }
    if (remaining.length > body!.length - offset) throw new ProcessorError("limit-exceeded");
    body!.set(remaining, offset);
    offset += remaining.length;
  }
  if (!parsed || offset !== body!.length) throw new ProcessorError("invalid-image");
  return { ...parsed, bytes: body! };
}

async function main() {
  const { limits, recipe, op, bytes } = await request();
  const memory = setInterval(() => {
    if (process.memoryUsage().rss > limits.maxMemoryBytes) process.exit(73);
  }, 10);
  try {
    let sharp: typeof import("sharp").default;
    try {
      sharp = (await import("sharp")).default;
      if (sharp.versions.sharp !== "0.35.5" || sharp.versions.vips !== "8.18.7")
        throw new Error("version");
      sharp.cache(false);
      sharp.concurrency(1);
    } catch {
      throw new ProcessorError("unavailable");
    }
    const format = signature(bytes);
    if (format === "jpeg" && (bytes.at(-2) !== 0xff || bytes.at(-1) !== 0xd9)) {
      throw new ProcessorError("invalid-image");
    }
    if (
      format === "webp" &&
      new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(4, true) + 8 !==
        bytes.length
    ) {
      throw new ProcessorError("invalid-image");
    }
    const decoderOptions = {
      failOn: "warning" as const,
      limitInputPixels: limits.maxPixels,
      sequentialRead: true,
    };
    const source = sharp(bytes, decoderOptions);
    const meta = await source.metadata();
    if (meta.format !== format) throw new ProcessorError("unsupported-image");
    const width = meta.width;
    const height = meta.pageHeight ?? meta.height;
    const png = format === "png" ? pngInfo(bytes) : null;
    const frames = Math.max(meta.pages ?? 1, png?.frames ?? 1);
    if (!width || !height) throw new ProcessorError("invalid-image");
    if (
      width > limits.maxWidth ||
      height > limits.maxHeight ||
      frames > limits.maxFrames ||
      width * height * frames > limits.maxPixels ||
      width * height * 4 + bytes.length > limits.maxMemoryBytes
    )
      throw new ProcessorError("limit-exceeded");
    // APNG is not decoded consistently by libvips. Reject all animations, including WebP.
    if (frames !== 1 || png?.animated || (format === "webp" && webpAnimated(bytes))) {
      throw new ProcessorError("unsupported-image");
    }
    // metadata() alone accepts truncated files. Force a complete strict pixel decode.
    await sharp(bytes, decoderOptions).raw().toBuffer();
    let output: Uint8Array = new Uint8Array(0);
    let metadata: ImageMetadata = {
      format,
      mime: `image/${format}`,
      width,
      height,
      orientation: meta.orientation ?? 1,
      hasAlpha: meta.hasAlpha,
      frames,
    };
    if (op === "transform") {
      if (!recipe) throw new ProcessorError("unavailable");
      let pipeline = sharp(bytes, decoderOptions)
        .autoOrient()
        .toColourspace("srgb")
        .resize({
          width: recipe.width,
          height: recipe.height,
          fit: recipe.fit,
          background: { r: 0, g: 0, b: 0, alpha: 0 },
        });
      if (recipe.format === "jpeg") pipeline = pipeline.flatten({ background: "#ffffff" });
      const encoder = pipeline.toFormat(recipe.format, { quality: recipe.quality });
      let info: import("sharp").OutputInfo | undefined;
      encoder.on("info", (value) => {
        info = value;
      });
      const chunks: Buffer[] = [];
      let size = 0;
      for await (const chunk of encoder) {
        size += chunk.length;
        if (size > limits.maxOutputBytes) {
          encoder.destroy();
          throw new ProcessorError("limit-exceeded");
        }
        chunks.push(chunk);
      }
      if (!info) throw new ProcessorError("unavailable");
      output = Buffer.concat(chunks, size);
      metadata = {
        format: recipe.format,
        mime: `image/${recipe.format}`,
        width: info.width,
        height: info.height,
        orientation: 1,
        hasAlpha: info.channels === 4,
        frames: 1,
      };
    }
    await Bun.write(Bun.stdout, JSON.stringify({ metadata, length: output.length }) + "\n");
    if (output.length) await Bun.write(Bun.stdout, output);
  } finally {
    clearInterval(memory);
  }
}

try {
  await main();
} catch (error) {
  const message = error instanceof Error ? error.message : "";
  const code =
    error instanceof ProcessorError
      ? error.code
      : /pixel limit/i.test(message)
        ? "limit-exceeded"
        : "invalid-image";
  await Bun.write(Bun.stdout, JSON.stringify({ code, length: 0 }) + "\n");
}
