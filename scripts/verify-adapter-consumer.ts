import {
  chmod,
  copyFile,
  mkdtemp,
  mkdir,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";

// Read-only staging registry: real package coordinates and tarballs, never npm publish.
const options = new Map<string, string>();
for (let i = 2; i < process.argv.length; i += 2) {
  const key = process.argv[i]!;
  const value = process.argv[i + 1];
  if (!["--verified", "--cli", "--output"].includes(key) || !value || options.has(key))
    throw Error("Use --verified verified.json --cli /path/to/lenso [--output receipt.json]");
  options.set(key, value);
}
if (!options.has("--verified") || !options.has("--cli")) throw Error("Missing required paths");
if (process.platform === "win32") throw Error("This language-adapter acceptance requires Unix");
const verified = await Bun.file(resolve(options.get("--verified")!)).json();
const root = await mkdtemp(join(tmpdir(), "lenso-registry-consumer-"));
const output = resolve(options.get("--output") ?? "output/release/adapter-consumer.json");
const requests: { method: string; path: string; candidate: boolean }[] = [];
const candidates = new Map<
  string,
  { manifest: any; data: Uint8Array<ArrayBuffer>; archive: string }
>();
const checks: Record<string, unknown>[] = [];
const env = Object.fromEntries(
  Object.entries(process.env).filter(
    ([key]) =>
      ![
        "BUN_OPTIONS",
        "NODE_PATH",
        "LENSO_TS_ROOT",
        "LENSO_TS_MODULE",
        "NPM_TOKEN",
        "NODE_AUTH_TOKEN",
        "BUN_AUTH_TOKEN",
        "GH_TOKEN",
        "GITHUB_TOKEN",
      ].includes(key),
  ),
);
const bins = join(root, "bin");
await mkdir(bins);
await symlink(process.execPath, join(bins, "bun"));
await copyFile(resolve(options.get("--cli")!), join(bins, "lenso"));
await chmod(join(bins, "lenso"), 0o755);
env.PATH = bins + ":/usr/bin:/bin";

async function command(cwd: string, args: string[], expected = 0, input?: string) {
  const child = Bun.spawn(args, {
    cwd,
    env,
    stdin: input === undefined ? "ignore" : "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  if (input !== undefined) {
    child.stdin!.write(input);
    child.stdin!.end();
  }
  const timeout = setTimeout(() => child.kill("SIGTERM"), 90000);
  try {
    const [code, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    if (code !== expected) throw Error(JSON.stringify({ args, code, expected, stdout, stderr }));
    return { code, stdout, stderr };
  } finally {
    clearTimeout(timeout);
  }
}
async function write(path: string, value: unknown) {
  await writeFile(path, JSON.stringify(value, null, 2) + "\n");
}
const server = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  fetch(request): Response {
    const url = new URL(request.url);
    const path = decodeURIComponent(url.pathname);
    const candidate = candidates.get(path.slice(1).replace(/\/$/, ""));
    const archive = [...candidates.values()].find((value) => path === "/tarballs/" + value.archive);
    requests.push({ method: request.method, path, candidate: !!(candidate || archive) });
    if (!["GET", "HEAD"].includes(request.method))
      return new Response("Read only", { status: 405 });
    if (archive)
      return new Response(archive.data, {
        headers: { "content-type": "application/octet-stream" },
      });
    if (candidate) {
      const manifest = {
        ...candidate.manifest,
        dist: {
          tarball: server.url.origin + "/tarballs/" + candidate.archive,
          integrity:
            "sha512-" + new Bun.CryptoHasher("sha512").update(candidate.data).digest("base64"),
          shasum: new Bun.CryptoHasher("sha1").update(candidate.data).digest("hex"),
        },
      };
      return Response.json({
        name: manifest.name,
        "dist-tags": { latest: manifest.version },
        versions: { [manifest.version]: manifest },
      });
    }
    return Response.redirect("https://registry.npmjs.org" + url.pathname + url.search, 302);
  },
});
try {
  for (const name of ["@lenso/engine", "@lenso/cli"]) {
    const entry = verified.packages.find((pkg: any) => pkg.name === name);
    if (!entry) throw Error("Missing verified package " + name);
    const archive = resolve(entry.archive);
    const data = new Uint8Array(await readFile(archive));
    if (new Bun.CryptoHasher("sha256").update(data).digest("hex") !== entry.sha256)
      throw Error("Archive checksum changed: " + name);
    const packed = JSON.parse(
      (await command(root, ["/usr/bin/tar", "-xOzf", archive, "package/package.json"])).stdout,
    );
    if (packed.name !== name || packed.version !== entry.version)
      throw Error("Archive identity mismatch");
    for (const group of ["dependencies", "optionalDependencies", "peerDependencies"])
      for (const range of Object.values(packed[group] ?? {}))
        if (/^(file:|link:|workspace:)/.test(String(range)))
          throw Error("Non-registry packed dependency");
    candidates.set(name, { manifest: packed, data, archive: basename(archive) });
  }
  const cli = candidates.get("@lenso/cli")!.manifest;
  const engine = candidates.get("@lenso/engine")!.manifest;
  if (
    cli.version !== "0.21.0" ||
    engine.version !== "0.7.0" ||
    cli.bin ||
    !cli.exports["./adapter"] ||
    !Bun.semver.satisfies(engine.version, cli.dependencies["@lenso/engine"])
  )
    throw Error("Unexpected adapter release contract");

  const negative = join(root, "current-npm");
  await mkdir(negative);
  await write(join(negative, "package.json"), {
    private: true,
    type: "module",
    dependencies: { "@lenso/cli": "0.20.1" },
  });
  await command(negative, [
    process.execPath,
    "install",
    "--ignore-scripts",
    "--registry",
    "https://registry.npmjs.org",
    "--cache-dir",
    join(root, "old-cache"),
  ]);
  const missingAdapter = await command(
    negative,
    [process.execPath, "-e", "await import('@lenso/cli/adapter')"],
    1,
  );
  const missingWorkspace = await command(
    negative,
    [
      process.execPath,
      "-e",
      "import {createWorkspaceSupervisor} from '@lenso/engine'; console.log(createWorkspaceSupervisor)",
    ],
    1,
  );
  if (
    !missingAdapter.stderr.includes("adapter") ||
    !missingWorkspace.stderr.includes("createWorkspaceSupervisor")
  )
    throw Error("Unexpected current-registry negative diagnostic");
  if (
    (await Bun.file(join(negative, "node_modules/@lenso/engine/package.json")).json()).version !==
    "0.6.0"
  )
    throw Error("Registry Engine baseline changed; refresh the release analysis");
  checks.push({
    check: "actual-current-npm-negative",
    cli: "0.20.1",
    engine: "0.6.0",
    adapterMissing: true,
    workspaceSupervisorMissing: true,
  });

  const app = join(root, "app");
  await mkdir(join(app, "src"), { recursive: true });
  const manifest = {
    private: true,
    type: "module",
    dependencies: {
      "@lenso/cli": cli.version,
      "@lenso/engine": engine.version,
      "@lenso/core": "0.3.1",
      zod: "4.6.5",
    },
    devDependencies: { typescript: "7.0.2", "@types/bun": "1.4.2" },
  };
  await write(join(app, "package.json"), manifest);
  await writeFile(
    join(app, "bunfig.toml"),
    '[install.scopes]\n"@lenso" = { url = "' + server.url.origin + '" }\n',
  );
  await command(app, [
    process.execPath,
    "install",
    "--ignore-scripts",
    "--registry",
    server.url.origin,
    "--cache-dir",
    join(root, "new-cache"),
  ]);
  await command(app, [
    process.execPath,
    "install",
    "--frozen-lockfile",
    "--ignore-scripts",
    "--registry",
    server.url.origin,
    "--cache-dir",
    join(root, "new-cache"),
  ]);
  await write(join(app, "tsconfig.json"), {
    compilerOptions: {
      target: "ES2022",
      module: "ESNext",
      moduleResolution: "Bundler",
      strict: true,
      skipLibCheck: true,
      types: ["bun"],
      noEmit: true,
    },
    include: ["lenso.config.ts", "src"],
  });
  await writeFile(
    join(app, "src/service.ts"),
    [
      'import {z} from "zod";',
      "export const input=z.object({name:z.string().trim().min(2)});",
      'export async function greet(value:z.output<typeof input>){return {message:"Hello, "+input.parse(value).name+"!"};}',
    ].join("\n"),
  );
  await writeFile(
    join(app, "lenso.config.ts"),
    [
      'import {defineApp,definePlugin} from "@lenso/core";',
      'import {writeFileSync} from "node:fs";',
      'import {defineOperation} from "@lenso/cli";',
      'import {greet,input} from "./src/service";',
      'const greeting=definePlugin({id:"greeting",setup(context){context.onCleanup(()=>Bun.write("cleaned","yes").then(()=>{}));writeFileSync("started","yes");return {greet};}});',
      "export default defineApp({plugins:[greeting]});",
      'export const operations=[defineOperation({plugin:greeting,method:"greet",input,description:"Greet a person",effect:"read"})];',
    ].join("\n"),
  );
  await writeFile(
    join(app, "src/server.ts"),
    [
      'import {reportDevReady} from "@lenso/engine/dev-ready";',
      'import {greet} from "./service";',
      'const server=Bun.serve({hostname:"127.0.0.1",port:0,async fetch(){return Response.json(await greet({name:"Ada"}));}});',
      'reportDevReady({urls:[server.url],capabilities:["greeting.greet"]});',
      'process.on("SIGTERM",()=>{server.stop(true)});',
    ].join("\n"),
  );
  await command(app, [
    process.execPath,
    "-e",
    [
      "import assert from 'node:assert/strict';",
      "const before=console.log;const module=await import('@lenso/cli/adapter');",
      "assert.equal(typeof module.runAdapter,'function');assert.equal(console.log,before);",
      "for(const name of ['@lenso/cli/adapter','@lenso/engine','@lenso/core'])assert.ok(Bun.resolveSync(name,process.cwd()).includes('/dist/'));",
      "assert.equal((await Bun.file('node_modules/@lenso/cli/package.json').json()).bin,undefined);",
      "assert.equal(await Bun.file('node_modules/.bin/lenso').exists(),false);",
      "assert.equal(await Bun.file('node_modules/@lenso/cli/dist/bin.js').exists(),false);",
    ].join("\n"),
  ]);
  await command(app, [process.execPath, join(app, "node_modules/typescript/bin/tsc"), "--noEmit"]);
  const inspected = JSON.parse(
    (await command(app, ["lenso", "inspect", "greeting", "greet", "--json"])).stdout,
  );
  if (!inspected.ok || (await Bun.file(join(app, "started")).exists()))
    throw Error("Inspect ran setup");
  const called = JSON.parse(
    (
      await command(
        app,
        ["lenso", "call", "greeting", "greet", "--stdin", "--json"],
        0,
        '{"name":"Ada"}',
      )
    ).stdout,
  );
  if (called.data.message !== "Hello, Ada!" || !(await Bun.file(join(app, "cleaned")).exists()))
    throw Error("Call/cleanup mismatch");
  for (const action of ["check", "generate", "build"])
    await command(app, ["lenso", action, "--json"]);
  checks.push({
    check: "registry-coordinate-install-typecheck-import-inspect-call-build",
    manifest,
    packedCliEngineRange: cli.dependencies["@lenso/engine"],
    usesFileOrWorkspaceOverrides: false,
    coreSource: "actual public npm @lenso/core@0.3.1",
    noTsBin: true,
  });

  const dev = Bun.spawn(["lenso", "dev", "--json", "--timeout-ms", "20000"], {
    cwd: app,
    env,
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  const diagnostic = new Response(dev.stderr).text();
  const urls: string[] = [];
  const events: any[] = [];
  let pending = "";
  let stopping = false;
  try {
    for await (const bytes of dev.stdout) {
      pending += new TextDecoder().decode(bytes);
      let index: number;
      while ((index = pending.indexOf("\n")) !== -1) {
        const event = JSON.parse(pending.slice(0, index));
        pending = pending.slice(index + 1);
        events.push(event);
        if (event.event === "result") {
          if (!stopping) throw Error(JSON.stringify({ event, stderr: await diagnostic }));
          continue;
        }
        if (event.event !== "ready") continue;
        const url = event.urls[0];
        urls.push(url);
        const result = await (await fetch(url)).json();
        if (result.message !== (urls.length === 1 ? "Hello, Ada!" : "Welcome, Ada!"))
          throw Error("HTTP restart content mismatch");
        if (urls.length === 1) {
          const source = join(app, "src/service.ts");
          await writeFile(source, (await readFile(source, "utf8")).replace("Hello, ", "Welcome, "));
        }
      }
      if (urls.length === 2 && !stopping) {
        stopping = true;
        dev.kill("SIGTERM");
      }
    }
    if (urls.length !== 2) throw Error("Missing watch restart");
    if ((await dev.exited) !== 143) throw Error("Unexpected dev cancellation");
    await diagnostic;
    for (const url of urls) {
      let closed = false;
      try {
        await fetch(url);
      } catch {
        closed = true;
      }
      if (!closed) throw Error("Owned listener survived");
    }
    checks.push({
      check: "released-lenso-dev-http-watch-restart-cleanup",
      readyCount: urls.length,
      firstResponse: "Hello, Ada!",
      restartedResponse: "Welcome, Ada!",
      listenersClosed: true,
      events,
    });
  } finally {
    if (dev.exitCode === null) {
      dev.kill("SIGTERM");
      await dev.exited;
    }
  }
  for (const candidate of candidates.values())
    if (
      !requests.some(
        (entry) => entry.path === "/tarballs/" + candidate.archive && entry.method === "GET",
      )
    )
      throw Error("Candidate archive was not fetched from staging registry");
  if (requests.some((entry) => !["GET", "HEAD"].includes(entry.method)))
    throw Error("Registry write attempted");
  if (Bun.which("cargo", { PATH: env.PATH }) || Bun.which("lenso-unified", { PATH: env.PATH }))
    throw Error("Acceptance PATH unexpectedly contains Rust or an old CLI");
  const installed = await Bun.file(join(app, "bun.lock")).text();
  if (
    installed.includes("workspace:") ||
    installed.includes("file:") ||
    installed.includes("link:")
  )
    throw Error("Consumer lockfile contains source links");
  await Bun.write(
    output,
    JSON.stringify(
      {
        mode: "read-only staging registry; candidates NOT published",
        bun: Bun.version,
        platform: process.platform + "/" + process.arch,
        sourceRootNotUsedByConsumer: true,
        rustOnPath: !!Bun.which("cargo", { PATH: env.PATH }),
        cliVersion: (await command(app, ["lenso", "--version"])).stdout.trim(),
        candidates: [...candidates.values()].map((value) => ({
          name: value.manifest.name,
          version: value.manifest.version,
          sha256: new Bun.CryptoHasher("sha256").update(value.data).digest("hex"),
        })),
        checks,
        requests,
        consumerManifest: manifest,
        consumerLockfile: installed,
      },
      null,
      2,
    ) + "\n",
  );
  console.log(output);
} finally {
  await Bun.write(output + ".requests.json", JSON.stringify(requests, null, 2) + "\n");
  server.stop(true);
  await rm(root, { recursive: true, force: true });
}
