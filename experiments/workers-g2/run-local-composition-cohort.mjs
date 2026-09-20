// Execute the real local-workerd cohort from clean candidate checkouts. This
// intentionally builds in a disposable mirror: qualifications must be able to
// prove their source closure without leaving generated artifacts or package
// manager state in the supplied candidate worktrees.
import { createHash } from "node:crypto";
import { execFileSync, spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";

const root = dirname(fileURLToPath(import.meta.url));
const manifest = JSON.parse(
  readFileSync(resolve(root, "local-composition-cohort.manifest.json")),
);

const optionNames = new Set([
  "--output",
  "--core-source",
  "--protocol-source",
  "--runtime-source",
  "--web-source",
  "--auth-source",
  "--cargo",
  "--wasm-bindgen",
  "--keep-workdir",
]);
const options = {};
for (let index = 2; index < process.argv.length; index += 1) {
  const name = process.argv[index];
  if (!optionNames.has(name) || name in options)
    throw Error(`unknown or duplicate option ${name}`);
  if (name === "--keep-workdir") {
    options[name] = true;
    continue;
  }
  const value = process.argv[++index];
  if (!value || value.startsWith("--")) throw Error(`${name} requires a value`);
  options[name] = value;
}
if (!options["--output"])
  throw Error("--output is required; receipts are never written into a candidate checkout");

const defaultRuntimeSource = resolve(root, "../..");
for (const name of [
  "--core-source",
  "--protocol-source",
  "--web-source",
  "--auth-source",
]) {
  if (!options[name]) throw Error(`${name} is required`);
}
const sourcePaths = {
  core: resolve(options["--core-source"]),
  protocol: resolve(options["--protocol-source"]),
  runtime: resolve(options["--runtime-source"] || defaultRuntimeSource),
  web: resolve(options["--web-source"]),
  auth: resolve(options["--auth-source"]),
};
const output = resolve(options["--output"]);
const cargoExecutable = options["--cargo"]
  ? resolve(options["--cargo"])
  : process.env.LENSO_CARGO || "cargo";
const wasmBindgen = options["--wasm-bindgen"]
  ? resolve(options["--wasm-bindgen"])
  : process.env.WASM_BINDGEN || "wasm-bindgen";

function hash(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function git(path, args) {
  return execFileSync("git", ["-C", path, ...args], { encoding: "utf8" }).trim();
}

function sourceIdentity(name, path, requiredFiles) {
  if (!existsSync(path)) throw Error(`${name} source does not exist: ${path}`);
  for (const file of requiredFiles) {
    if (!existsSync(resolve(path, file)))
      throw Error(`${name} source is missing ${file}`);
  }
  const status = git(path, ["status", "--porcelain"]);
  if (status) throw Error(`${name} source must be clean before qualification`);
  return {
    revision: git(path, ["rev-parse", "HEAD"]),
    tree: git(path, ["rev-parse", "HEAD^{tree}"]),
    clean: true,
    requiredFileSha256: Object.fromEntries(
      requiredFiles.map((file) => [file, hash(resolve(path, file))]),
    ),
  };
}

function run(name, command, args, cwd, environment = {}) {
  const startedAt = new Date().toISOString();
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    timeout: 180_000,
    env: { ...process.env, ...environment },
  });
  const record = {
    name,
    command: [command, ...args],
    workingDirectory: cwd,
    startedAt,
    finishedAt: new Date().toISOString(),
    exitCode: result.status,
  };
  if (result.error || result.status !== 0) {
    const detail = String(result.error || result.stderr || result.stdout || "unknown failure")
      .replaceAll(/(?:https?:\/\/[^\s]+|Bearer\s+\S+)/g, "[redacted]")
      .slice(-4000);
    throw Object.assign(Error(`${name} failed: ${detail}`), { record });
  }
  return { ...record, stdout: result.stdout || "" };
}

function copyDirectory(source, destination) {
  cpSync(source, destination, {
    recursive: true,
    filter(path) {
      return ![
        "node_modules",
        "pkg",
        "target",
        ".oauth-postgres",
        ".w02",
        ".wrangler",
        "evidence",
      ].includes(path.split("/").at(-1));
    },
  });
}

function writeCargoConfig(path) {
  const packagePaths = {
    "lenso-app-plan": join(sourcePaths.core, "crates/lenso-app-plan"),
    "lenso-kernel": join(sourcePaths.core, "crates/lenso-kernel"),
    "lenso-plugin-authoring": join(sourcePaths.protocol, "crates/lenso-plugin-authoring"),
    lenso: join(sourcePaths.runtime, "crates/lenso"),
    "lenso-native-adapter": join(sourcePaths.runtime, "crates/lenso-native-adapter"),
    "lenso-native-adapter-macros": join(sourcePaths.runtime, "crates/lenso-native-adapter-macros"),
    "lenso-runtime-codec": join(sourcePaths.runtime, "crates/lenso-runtime-codec"),
    "lenso-workers-driver": join(sourcePaths.runtime, "crates/lenso-workers-driver"),
    "lenso-capability-http-client": join(sourcePaths.web, "crates/lenso-capability-http-client"),
    "lenso-capability-http-endpoint": join(sourcePaths.web, "crates/lenso-capability-http-endpoint"),
    "lenso-capability-http-stream-endpoint": join(sourcePaths.web, "crates/lenso-capability-http-stream-endpoint"),
    "lenso-capability-websocket-endpoint": join(sourcePaths.web, "crates/lenso-capability-websocket-endpoint"),
    "lenso-web-ingress-plugin": join(sourcePaths.web, "crates/lenso-web-ingress-plugin"),
    "lenso-http-egress-plugin": join(sourcePaths.web, "crates/lenso-http-egress-plugin"),
  };
  for (const [name, source] of Object.entries(packagePaths)) {
    if (!existsSync(source)) throw Error(`source closure is missing ${name}`);
  }
  writeFileSync(
    path,
    `[patch.crates-io]\n${Object.entries(packagePaths)
      .map(([name, source]) => `${name} = { path = ${JSON.stringify(source)} }`)
      .join("\n")}\n`,
  );
}

function requirePassingCases(evidence, names, label) {
  if (evidence?.passed !== true) throw Error(`${label} did not pass`);
  const passed = new Set(
    evidence.cases?.filter((entry) => entry.passed).map((entry) => entry.name),
  );
  for (const name of names) {
    if (!passed.has(name)) throw Error(`${label} lacks passing case ${name}`);
  }
}

function evidenceLine(stdout, prefix, label) {
  const line = stdout.split("\n").find((entry) => entry.startsWith(prefix));
  if (!line) throw Error(`${label} did not emit ${prefix.trim()}`);
  return JSON.parse(line.slice(prefix.length));
}

function mirrorNodePackage(source, destination) {
  rmSync(destination, { recursive: true, force: true });
  mkdirSync(dirname(destination), { recursive: true });
  copyDirectory(source, destination);
}

function normalizeG2Mirror(g2) {
  const nativeAdapterVersion = readFileSync(
    join(sourcePaths.runtime, "crates/lenso-native-adapter/Cargo.toml"),
    "utf8",
  ).match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  const workersDriverVersion = readFileSync(
    join(sourcePaths.runtime, "crates/lenso-workers-driver/Cargo.toml"),
    "utf8",
  ).match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (!nativeAdapterVersion || !workersDriverVersion)
    throw Error("Runtime crate versions are required for the disposable G2 mirror");
  const workspaceManifest = join(g2, "Cargo.toml");
  writeFileSync(
    workspaceManifest,
    readFileSync(workspaceManifest, "utf8").replace(
      /\n\[patch\.crates-io\][\s\S]*$/,
      "\n",
    ).replace(
      'lenso-native-adapter = { path = "../../crates/lenso-native-adapter" }',
      `lenso-native-adapter = "=${nativeAdapterVersion}"`,
    ),
  );
  const hostManifest = join(g2, "host/Cargo.toml");
  writeFileSync(
    hostManifest,
    readFileSync(hostManifest, "utf8").replace(
      /lenso-workers-driver = \{ version = "0\.1\.0", path = "\.\.\/\.\.\/\.\.\/crates\/lenso-workers-driver" \}/,
      `lenso-workers-driver = "=${workersDriverVersion}"`,
    ),
  );
}

function linkLockedNodeTooling(name, source, destination) {
  const nodeModules = join(source, "node_modules");
  const bin = join(nodeModules, ".pnpm/node_modules/.bin");
  for (const executable of ["esbuild", "workerd"]) {
    if (!existsSync(join(bin, executable))) {
      throw Error(
        `${name} needs locked node tooling; run pnpm install --frozen-lockfile in ${source} first`,
      );
    }
  }
  mkdirSync(destination, { recursive: true });
  symlinkSync(join(nodeModules, ".pnpm"), join(destination, ".pnpm"));
  if (existsSync(join(nodeModules, ".bin")))
    symlinkSync(join(nodeModules, ".bin"), join(destination, ".bin"));
  return run(`${name}-workerd-version`, join(bin, "workerd"), ["--version"], source);
}

const identities = {
  core: sourceIdentity("Core", sourcePaths.core, [
    "crates/lenso-app-plan/Cargo.toml",
    "crates/lenso-kernel/Cargo.toml",
  ]),
  protocol: sourceIdentity("Protocol", sourcePaths.protocol, [
    "crates/lenso-plugin-authoring/Cargo.toml",
    "crates/lenso-process-protocol/Cargo.toml",
  ]),
  runtime: sourceIdentity("Runtime", sourcePaths.runtime, [
    "packages/workers-runtime/build.mjs",
    "experiments/workers-g2/run-target-local-qualification.mjs",
    "experiments/workers-g2/run-workerd-qualification.mjs",
  ]),
  web: sourceIdentity("Web", sourcePaths.web, [
    "crates/lenso-web-ingress-plugin/Cargo.toml",
    "crates/lenso-capability-http-endpoint/Cargo.toml",
  ]),
  auth: sourceIdentity("Auth", sourcePaths.auth, [
    "crates/lenso-auth-oauth-flow-plugin/src/lib.rs",
    "crates/lenso-auth-oauth-flow-plugin/src/postgres_transport.rs",
    "experiments/workers-g4/oauth-postgres-workerd.mjs",
  ]),
};
const cargoCommand = run("cargo-version", cargoExecutable, ["+1.94.0", "--version"], root);
const bindgenCommand = run("wasm-bindgen-version", wasmBindgen, ["--version"], root);
const bindgenVersion = bindgenCommand.stdout.trim();
if (bindgenVersion !== "wasm-bindgen 0.2.127")
  throw Error(`expected wasm-bindgen 0.2.127, got ${bindgenVersion}`);

const temporary = mkdtempSync(join(tmpdir(), "lenso-workers-local-composition-"));
const commands = [cargoCommand, bindgenCommand];
try {
  const config = join(temporary, "source-closure.toml");
  const cargo = join(temporary, "cargo-source-closure.sh");
  writeCargoConfig(config);
  writeFileSync(
    cargo,
    `#!/bin/sh\nunset CARGO\nif [ "\${1#\\+}" != "$1" ]; then\n  toolchain="$1"\n  shift\n  exec ${JSON.stringify(cargoExecutable)} "$toolchain" --config ${JSON.stringify(config)} "$@"\nfi\nexec ${JSON.stringify(cargoExecutable)} --config ${JSON.stringify(config)} "$@"\n`,
  );
  execFileSync("chmod", ["755", cargo]);

  const runtimeMirror = join(temporary, "runtime");
  const g2 = join(runtimeMirror, "experiments/workers-g2");
  mkdirSync(join(runtimeMirror, "experiments"), { recursive: true });
  copyDirectory(join(sourcePaths.runtime, "experiments/workers-g2"), g2);
  normalizeG2Mirror(g2);
  mkdirSync(join(runtimeMirror, "packages"), { recursive: true });
  copyDirectory(join(sourcePaths.runtime, "packages/workers-runtime"), join(runtimeMirror, "packages/workers-runtime"));
  commands.push(
    linkLockedNodeTooling(
      "g2",
      join(sourcePaths.runtime, "experiments/workers-g2"),
      join(g2, "node_modules"),
    ),
  );
  mirrorNodePackage(
    join(sourcePaths.runtime, "packages/workers-runtime"),
    join(g2, "node_modules/@lenso/workers-runtime"),
  );
  const cohortEnvironment = {
    CARGO: cargo,
    CARGO_NET_OFFLINE: "true",
    WASM_BINDGEN: wasmBindgen,
    LENSO_QUALIFICATION_SOURCE_REVISION: identities.runtime.revision,
  };
  commands.push(run("g2-build", "bash", ["build.sh"], g2, cohortEnvironment));
  const w02Path = join(temporary, "w02.json.gz");
  const targetPath = join(temporary, "target.json.gz");
  commands.push(run("g2-w02-local-workerd", process.execPath, ["run-workerd-qualification.mjs", w02Path], g2, cohortEnvironment));
  commands.push(run("g2-target-local-workerd", process.execPath, ["run-target-local-qualification.mjs", targetPath], g2, cohortEnvironment));
  const w02 = JSON.parse(gunzipSync(readFileSync(w02Path)).toString("utf8"));
  const target = JSON.parse(gunzipSync(readFileSync(targetPath)).toString("utf8"));
  requirePassingCases(
    w02,
    manifest.subcohorts.find((entry) => entry.id === "w02-stream-session").requiredCases,
    "W02 local workerd evidence",
  );
  requirePassingCases(
    target,
    manifest.subcohorts.find((entry) => entry.id === "g2-web-ingress").requiredCases,
    "G2 Web ingress local workerd evidence",
  );
  if (w02.artifact?.wasmSha256 !== target.identity?.wasmSha256)
    throw Error("G2 W02 and target evidence use different Wasm artifacts");
  if (w02.artifact?.sourceSha256 !== target.identity?.sourceSha256)
    throw Error("G2 W02 and target evidence use different source inventories");

  const authMirror = join(temporary, "auth");
  const g4 = join(authMirror, "experiments/workers-g4");
  mkdirSync(join(authMirror, "experiments"), { recursive: true });
  copyDirectory(join(sourcePaths.auth, "experiments/workers-g4"), g4);
  symlinkSync(join(sourcePaths.auth, "crates"), join(authMirror, "crates"));
  symlinkSync(join(sourcePaths.auth, "workers"), join(authMirror, "workers"));
  symlinkSync(join(sourcePaths.auth, "Cargo.toml"), join(authMirror, "Cargo.toml"));
  commands.push(
    linkLockedNodeTooling(
      "auth",
      join(sourcePaths.auth, "experiments/workers-g4"),
      join(g4, "node_modules"),
    ),
  );
  mirrorNodePackage(
    join(sourcePaths.runtime, "packages/workers-runtime"),
    join(g4, "node_modules/@lenso/workers-runtime"),
  );
  mirrorNodePackage(
    join(sourcePaths.web, "crates/lenso-http-egress-plugin/js"),
    join(g4, "node_modules/@lenso/http-egress-workers"),
  );
  const authRun = run(
    "auth-oauth-local-workerd",
    process.execPath,
    ["qualify-oauth-postgres-workerd.mjs"],
    g4,
    cohortEnvironment,
  );
  commands.push(authRun);
  const auth = evidenceLine(
    authRun.stdout,
    "AUTH_POSTGRES_COHORT_EVIDENCE ",
    "Auth OAuth local workerd cohort",
  );
  requirePassingCases(
    auth,
    manifest.subcohorts.find((entry) => entry.id === "auth-oauth-capability").requiredCases,
    "Auth OAuth Capability local workerd evidence",
  );

  const report = {
    schema: "lenso-workers-local-composition-cohort-report-v1",
    status: manifest.resultStatus,
    environment: manifest.environment,
    manifest: {
      schema: manifest.schema,
      sha256: hash(resolve(root, "local-composition-cohort.manifest.json")),
    },
    sources: identities,
    commands: commands.map(({ stdout: _stdout, ...record }) => record),
    localWorkerd: {
      g2: {
        artifact: {
          sourceSha256: w02.artifact.sourceSha256,
          wasmSha256: w02.artifact.wasmSha256,
          glueSha256: w02.artifact.glueSha256,
          workerd: w02.artifact.workerd,
        },
        webIngressCases: target.cases.filter((entry) => entry.passed).map((entry) => entry.name),
        streamSessionCases: w02.cases.filter((entry) => entry.passed).map((entry) => entry.name),
      },
      auth: {
        schema: auth.schema,
        cases: auth.cases.filter((entry) => entry.passed).map((entry) => entry.name),
        directWasmCapabilityOnly: true,
        assertion: "Auth subcohort uses a real workerd test and generated Auth Wasm, but does not claim HTTP ingress.",
      },
    },
    compositionBoundary: "The three subcohorts share exact candidate source closure and a local workerd runtime, but are intentionally separate host compositions. This report does not claim one fused product App.",
    doesNotProve: manifest.doesNotProve,
  };
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, JSON.stringify(report, null, 2) + "\n");
  console.log(
    "LOCAL_WORKER_COMPOSITION_COHORT " +
      JSON.stringify({
        status: report.status,
        output,
        outputSha256: hash(output),
        sources: Object.fromEntries(
          Object.entries(identities).map(([name, identity]) => [name, identity.revision]),
        ),
      }),
  );
} finally {
  if (options["--keep-workdir"]) console.error(`kept local cohort workdir: ${temporary}`);
  else rmSync(temporary, { recursive: true, force: true });
}
