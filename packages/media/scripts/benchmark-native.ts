import { cpus, platform, arch, release } from "node:os";
import sharp from "sharp";
import { createBunImageProcessor } from "../src/bun";
import { defaultProcessorLimits, type Recipe } from "../src/processor";
import { fixture } from "../test/native-fixtures";

const bytes = await fixture(1920, 1080);
const recipe: Recipe = {
  width: 320,
  height: 320,
  fit: "cover",
  format: "webp",
  quality: 80,
  metadata: "strip",
  animation: "reject",
};
const concurrency = 2;
const processor = createBunImageProcessor({ limits: { concurrency } });
const signal = () => new AbortController().signal;
await processor.inspect(bytes, signal());
await processor.transform(bytes, recipe, signal());
async function measure(operation: () => Promise<unknown>, iterations: number) {
  const timings: number[] = [];
  for (let i = 0; i < iterations; i++) {
    const start = performance.now();
    await operation();
    timings.push(performance.now() - start);
  }
  return {
    iterations,
    milliseconds: timings,
    meanMs: timings.reduce((a, b) => a + b, 0) / timings.length,
  };
}
const inspect = await measure(() => processor.inspect(bytes, signal()), 5);
const transform = await measure(() => processor.transform(bytes, recipe, signal()), 5);
const batchStart = performance.now();
for (let i = 0; i < 3; i++) {
  await Promise.all(
    Array.from({ length: concurrency }, () => processor.transform(bytes, recipe, signal())),
  );
}
const batchMs = performance.now() - batchStart;
// Directly measure the same fixed child protocol. Bun's subprocess maxRSS is bytes,
// unlike Node's process.resourceUsage().maxRSS (KiB). No ps sampling is used here.
const child = Bun.spawn(
  [
    process.execPath,
    "--no-install",
    "--no-env-file",
    new URL("../src/sharp-child.ts", import.meta.url).pathname,
  ],
  {
    env: {
      VIPS_CONCURRENCY: "1",
      VIPS_DISC_THRESHOLD: String(defaultProcessorLimits.maxMemoryBytes),
    },
    stdin: "pipe",
    stdout: "pipe",
    stderr: "ignore",
  },
);
const output = new Response(child.stdout).arrayBuffer();
await child.stdin.write(
  JSON.stringify({
    op: "transform",
    limits: defaultProcessorLimits,
    recipe,
    length: bytes.length,
  }) + "\n",
);
await child.stdin.write(bytes);
await child.stdin.end();
const protocolBytes = new Uint8Array(await output);
if ((await child.exited) !== 0) throw new Error("Benchmark child failed");
const newline = protocolBytes.indexOf(10);
const header = JSON.parse(new TextDecoder().decode(protocolBytes.subarray(0, newline)));
if (header.code) throw new Error(`Benchmark failed: ${header.code}`);
console.log(
  JSON.stringify(
    {
      environment: {
        bun: Bun.version,
        os: platform(),
        architecture: arch(),
        release: release(),
        cpu: cpus()[0]?.model,
        sharp: sharp.versions.sharp,
        vips: sharp.versions.vips,
      },
      fixture: {
        width: 1920,
        height: 1080,
        inputBytes: bytes.length,
        outputBytes: header.length,
        recipe,
      },
      processorVersion: processor.version,
      concurrency,
      inspect,
      transform,
      batch: { operations: 6, milliseconds: batchMs },
      childMaxRSSBytes: child.resourceUsage()?.maxRSS,
      maxRSSMethod:
        "Bun Subprocess.resourceUsage().maxRSS after exit (bytes; OS high-water mark, one transform child)",
      parentMaxRSSBytes: process.resourceUsage().maxRSS * 1024,
      limits: defaultProcessorLimits,
    },
    null,
    2,
  ),
);
