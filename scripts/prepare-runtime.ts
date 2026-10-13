import { createHash } from "node:crypto";
import {
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import pinnedSnapshot from "./runtime-sdk.json";

const owner = "lenso-runtime-sdk-preparation-v1\n";
const maxArchiveBytes = 20 * 1024 * 1024;
const downloadTimeoutMs = 60_000;
type Pin = typeof pinnedSnapshot;
type Run = (args: string[], cwd: string) => Promise<void>;
export type ArchiveFetcher = (url: string, options?: RequestInit) => Promise<Response>;

async function exists(path: string) {
  try {
    return await lstat(path);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
    throw error;
  }
}

function sha256(bytes: Uint8Array) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function download(pin: Pin, fetcher: ArchiveFetcher) {
  if (
    pin.repoURL !== "https://github.com/LioRael/lenso-runtime" ||
    !/^[a-f0-9]{40}$/.test(pin.commit) ||
    !/^[a-f0-9]{64}$/.test(pin.hash) ||
    pin.version !== "0.1.0"
  ) {
    throw new Error("Invalid Runtime SDK snapshot pin");
  }
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), downloadTimeoutMs);
  let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  try {
    const response = await fetcher(
      `https://codeload.github.com/LioRael/lenso-runtime/tar.gz/${pin.commit}`,
      { signal: controller.signal, redirect: "error" },
    );
    if (!response.ok || !response.body) throw new Error("Runtime SDK archive download failed");
    if (Number(response.headers.get("content-length")) > maxArchiveBytes) {
      throw new Error("Runtime SDK archive exceeds byte limit");
    }
    reader = response.body.getReader();
    const chunks: Uint8Array[] = [];
    let size = 0;
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > maxArchiveBytes) throw new Error("Runtime SDK archive exceeds byte limit");
      chunks.push(value);
    }
    const bytes = Buffer.concat(chunks, size);
    if (sha256(bytes) !== pin.hash) throw new Error("Runtime SDK archive checksum mismatch");
    return bytes;
  } finally {
    controller.abort();
    await reader?.cancel().catch(() => {});
    clearTimeout(timeout);
  }
}

const run: Run = async (args, cwd) => {
  const child = Bun.spawn([process.execPath, ...args], {
    cwd,
    stdin: "ignore",
    stdout: "ignore",
    stderr: "ignore",
  });
  if ((await child.exited) !== 0) {
    throw new Error(`Runtime SDK command failed: bun ${args.join(" ")}`);
  }
};

async function fileHashes(root: string, prefix = ""): Promise<Record<string, string>> {
  const hashes: Record<string, string> = {};
  const entries = await readdir(join(root, prefix), { withFileTypes: true });
  entries.sort((a, b) => a.name.localeCompare(b.name, "en"));
  for (const entry of entries) {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) Object.assign(hashes, await fileHashes(root, relative));
    else if (entry.isFile()) hashes[relative] = sha256(await readFile(join(root, relative)));
    else throw new Error("Runtime SDK output must contain only regular files and directories");
  }
  return hashes;
}

// Dependency injection is for in-process fixtures only; the CLI has no pin/source override.
export async function prepareRuntime(
  root = resolve(import.meta.dir, ".."),
  dependencies: { fetch?: ArchiveFetcher; run?: Run; pin?: Pin } = {},
) {
  root = resolve(root);
  const manifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
  if (manifest.name !== "lenso-workspace" || manifest.private !== true) {
    throw new Error("Preparation root must be the consuming lenso-workspace");
  }
  const expectedBun = manifest.engines?.bun ?? "1.4.3";
  if (Bun.version !== expectedBun) {
    throw new Error(`Runtime SDK preparation requires Bun ${expectedBun}`);
  }
  const pin = dependencies.pin ?? pinnedSnapshot;
  // Authenticate the complete response before extracting files or executing snapshot code.
  const bytes = await download(pin, dependencies.fetch ?? fetch);
  const output = join(root, ".runtime-deps");
  const existing = await exists(output);
  if (existing) {
    const marker = await exists(join(output, ".owner"));
    if (
      !existing.isDirectory() ||
      existing.isSymbolicLink() ||
      !marker?.isFile() ||
      marker.isSymbolicLink() ||
      (await readFile(join(output, ".owner"), "utf8")) !== owner
    ) {
      throw new Error("Refusing to replace unowned .runtime-deps");
    }
  } else {
    await mkdir(output);
    await writeFile(join(output, ".owner"), owner, { flag: "wx" });
  }
  const lock = join(output, ".prepare-lock");
  await mkdir(lock); // An existing lock fails closed; never remove another invocation's lock.
  let temporary: string | undefined;
  try {
    for (const name of ["source", "sdk", "provenance.json"]) {
      const stat = await exists(join(output, name));
      if (
        stat &&
        (stat.isSymbolicLink() ||
          (name === "provenance.json" ? !stat.isFile() : !stat.isDirectory()))
      ) {
        throw new Error(`Refusing to replace unexpected .runtime-deps/${name}`);
      }
    }
    temporary = await mkdtemp(join(output, ".prepare-"));
    const source = join(temporary, "source");
    await mkdir(source);
    const archive = new Bun.Archive(Bun.gunzipSync(bytes));
    const prefix = `lenso-runtime-${pin.commit}/`;
    for (const [name, file] of await archive.files()) {
      const relative = name.slice(prefix.length);
      if (
        !name.startsWith(prefix) ||
        !relative ||
        relative.split("/").some((part) => part === ".." || part === "." || part === "") ||
        relative.includes("\\")
      ) {
        throw new Error("Unexpected Runtime SDK archive path");
      }
      const destination = join(source, relative);
      await mkdir(dirname(destination), { recursive: true });
      await writeFile(destination, await file.bytes());
    }
    const execute = dependencies.run ?? run;
    await execute(["install", "--frozen-lockfile", "--ignore-scripts"], source);
    await execute(["run", "build:sdk"], source);
    const sdkSource = join(source, "sdk/typescript");
    const sdkManifest = JSON.parse(await readFile(join(sdkSource, "package.json"), "utf8"));
    if (
      sdkManifest.name !== "@lenso/runtime-sdk" ||
      sdkManifest.version !== pin.version ||
      sdkManifest.private
    ) {
      throw new Error("Built Runtime SDK does not match the public package pin");
    }
    const sdk = join(temporary, "sdk");
    await mkdir(sdk);
    delete sdkManifest.scripts;
    await writeFile(join(sdk, "package.json"), `${JSON.stringify(sdkManifest, null, 2)}\n`);
    await cp(join(sdkSource, "dist"), join(sdk, "dist"), { recursive: true, dereference: false });
    const hashes = await fileHashes(sdk);
    for (const file of ["dist/index.js", "dist/index.d.ts", "dist/schema.js", "dist/schema.d.ts"]) {
      if (!hashes[file]) throw new Error(`Built Runtime SDK is missing ${file}`);
    }
    await writeFile(
      join(temporary, "provenance.json"),
      `${JSON.stringify({ ...pin, bun: Bun.version, files: hashes }, null, 2)}\n`,
    );
    // Stage both source and package before replacing any previous ready state.
    const moved: string[] = [];
    const installed: string[] = [];
    try {
      for (const name of ["source", "sdk", "provenance.json"]) {
        if (await exists(join(output, name))) {
          await rename(join(output, name), join(temporary, `previous-${name}`));
          moved.push(name);
        }
        await rename(join(temporary, name), join(output, name));
        installed.push(name);
      }
    } catch (error) {
      for (const name of installed.reverse()) {
        await rename(join(output, name), join(temporary, name));
      }
      for (const name of moved.reverse()) {
        await rename(join(temporary, `previous-${name}`), join(output, name));
      }
      throw error;
    }
  } finally {
    if (temporary) await rm(temporary, { recursive: true, force: true });
    await rm(lock, { recursive: true });
  }
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  if (args.length !== 0 && (args.length !== 2 || args[0] !== "--root")) {
    console.error("Usage: bun scripts/prepare-runtime.ts [--root <lenso-workspace>]");
    process.exitCode = 1;
  } else {
    try {
      await prepareRuntime(args[1]);
      console.log(
        "Runtime SDK 0.1.0 prepared in .runtime-deps/sdk; source in .runtime-deps/source",
      );
    } catch (error) {
      console.error(error instanceof Error ? error.message : "Runtime SDK preparation failed");
      process.exitCode = 1;
    }
  }
}
