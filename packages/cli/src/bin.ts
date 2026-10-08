#!/usr/bin/env bun
import { resolve } from "node:path";
import { build, discover, generate } from "@lenso/engine";
import { call, inspect } from "./engine";
import { dev } from "./dev";
import { SpanStatusCode, trace } from "@opentelemetry/api";
import {
  CliError,
  diagnostic,
  environmentSecrets,
  exitCode,
  redact,
  stableJson,
} from "./diagnostics";

const args = process.argv.slice(2);
const jsonMode = args.includes("--json");
const stdout = process.stdout.write.bind(process.stdout);
const secrets = environmentSecrets();
// Trusted application console output is routed to stderr during this CLI process.
for (const level of ["log", "info", "warn", "error", "debug"] as const) {
  console[level] = (...values: unknown[]) => {
    const safe = values.map((value) =>
      value instanceof Error ? "[Application error text omitted]" : redact(value, secrets),
    );
    process.stderr.write(
      `${safe.map((value) => (typeof value === "string" ? value : JSON.stringify(value))).join(" ")}\n`,
    );
  };
}
function usage(message: string): never {
  throw new CliError({ code: "invalid-arguments", phase: "arguments", message }, 2);
}
const help = {
  commands: [
    {
      name: "check",
      usage: "check [--root directory]",
      effect: "runs trusted Engine setup/discovery; validates assembly; no application setup",
    },
    {
      name: "inspect",
      usage: "inspect [plugin-id [method]] [--root directory]",
      effect: "describes explicit operations; no setup",
    },
    {
      name: "generate",
      usage: "generate [--root directory]",
      effect: "writes framework-owned .lenso entries",
    },
    {
      name: "build",
      usage: "build [--root directory] [--entry file]",
      effect: "generates entries and writes dist",
    },
    {
      name: "call",
      usage:
        "call <plugin-id> <method> [JSON input | --input-file file | --stdin] [--root directory]",
      effect: "validates input; starts app; invokes declared service operation; stops app",
    },
    {
      name: "dev",
      usage: "dev [--root directory] [--entry file]",
      effect: "watches source; supervises owned processes; human mode only",
    },
    { name: "help", usage: "help", effect: "describes commands; no config import" },
  ],
  json: "Add --json to finite commands. One schemaVersion=1 result on stdout; logs on stderr.",
  exitCodes: { success: 0, runtime: 1, argumentsOrInput: 2, discoveryOrAssembly: 3 },
  errorCodes: [
    "invalid-arguments",
    "input-read-failed",
    "invalid-json",
    "invalid-input",
    "config-load-failed",
    "invalid-config",
    "invalid-assembly",
    "duplicate-id",
    "missing-dependency",
    "cyclic-dependency",
    "invalid-id",
    "invalid-source",
    "invalid-operations",
    "duplicate-operation",
    "unknown-plugin",
    "unknown-operation",
    "unavailable-operation",
    "initialization-failed",
    "invocation-failed",
    "cleanup-failed",
    "invocation-and-cleanup-failed",
    "build-failed",
    "serialization-failed",
    "engine-config-load-failed",
    "invalid-engine-config",
    "invalid-engine-plugin",
    "duplicate-engine-plugin",
    "invalid-engine-order",
    "missing-engine-order",
    "cyclic-engine-order",
    "engine-capability-conflict",
    "invalid-engine-capability",
    "late-engine-registration",
    "invalid-engine-cleanup",
    "invalid-engine-convention",
    "invalid-engine-sources",
    "invalid-engine-watch",
    "engine-hook-failed",
    "unknown-engine-target",
    "invalid-generated-file",
    "generated-file-conflict",
    "unsafe-engine-output",
    "invalid-generated-ownership",
    "generated-file-modified",
    "invalid-build-entry",
    "invalid-build-output",
    "build-diagnostic",
    "engine-cleanup-failed",
    "engine-and-cleanup-failed",
    "engine-worker-exited",
    "engine-worker-timeout",
    "engine-worker-closed",
    "dev-entry-missing",
    "dev-runtime-timeout",
    "engine-session-closed",
    "invalid-engine-state",
  ],
  boundaries: [
    "Trusted local config/plugins; not a sandbox.",
    "Use shared input schemas and service authorization. CLI does not impersonate an HTTP actor.",
    "Direct stdout writes from trusted application code must be avoided; console is routed to stderr.",
    "Operation effects are descriptive; no retry, eval, authorization bypass or automatic cancellation.",
  ],
};

await trace.getTracer("@lenso/cli").startActiveSpan("lenso.cli.command", async (span) => {
  try {
    let command = "help";
    if (args[0] && !args[0].startsWith("-")) command = args.shift()!;
    const values = new Map<string, string>();
    const flags = new Set<string>();
    const positionals: string[] = [];
    for (let index = 0; index < args.length; index++) {
      const arg = args[index]!;
      if (["--json", "--stdin", "--help", "-h"].includes(arg)) {
        if (flags.has(arg)) usage("Repeated flag.");
        flags.add(arg);
      } else if (["--root", "--entry", "--input-file"].includes(arg)) {
        if (values.has(arg)) usage("Repeated option.");
        const value = args[++index];
        if (!value || value.startsWith("-")) usage("Missing option value.");
        values.set(arg, value);
      } else if (arg.startsWith("-")) usage("Unknown flag.");
      else positionals.push(arg);
    }
    if (flags.has("--help") || flags.has("-h")) command = "help";
    if (!help.commands.some((item) => item.name === command))
      usage("Unknown command; use lenso help.");
    span.setAttribute("lenso.cli.command", command);
    if (values.has("--entry") && !["build", "dev"].includes(command))
      usage("--entry is available for build/dev only.");
    if ((values.has("--input-file") || flags.has("--stdin")) && command !== "call")
      usage("Input options are available for call only.");
    if (command !== "call" && command !== "inspect" && positionals.length)
      usage("Unexpected positional arguments.");
    const root = values.get("--root") ?? process.cwd();
    const entry = values.get("--entry");
    let data: unknown;
    switch (command) {
      case "check": {
        const discovery = await discover(root);
        data = {
          valid: true,
          configPath: discovery.configPath,
          order: discovery.ordered.map((plugin) => plugin.id),
        };
        break;
      }
      case "inspect":
        if (positionals.length > 2) usage("Usage: inspect [plugin-id [method]].");
        data = await inspect(root, positionals[0], positionals[1]);
        break;
      case "generate": {
        const manifest = await generate(root);
        data = {
          plugins: manifest.map((plugin) => plugin.id),
          directory: resolve(root, ".lenso"),
          files: (await Bun.file(resolve(root, ".lenso/.engine-files.json")).json()).files.map(
            (file: { path: string }) => file.path,
          ),
        };
        break;
      }
      case "build":
        data = { directory: await build(root, entry) };
        break;
      case "call": {
        if (positionals.length < 2 || positionals.length > 3)
          usage("Usage: call <plugin-id> <method> [JSON input].");
        const [plugin, method, inline] = positionals;
        const inputFile = values.get("--input-file");
        const inputCount =
          Number(inline !== undefined) +
          Number(inputFile !== undefined) +
          Number(flags.has("--stdin"));
        if (inputCount > 1)
          usage("Choose one input source: positional JSON, --input-file, or --stdin.");
        let inputText = inline ?? "{}";
        try {
          if (inputFile) inputText = await Bun.file(resolve(inputFile)).text();
          if (flags.has("--stdin")) inputText = await Bun.stdin.text();
        } catch {
          throw new CliError(
            {
              code: "input-read-failed",
              phase: "input",
              message: "Cannot read JSON input source.",
            },
            2,
          );
        }
        let input;
        try {
          input = JSON.parse(inputText);
        } catch {
          throw new CliError(
            { code: "invalid-json", phase: "input", message: "Input must be valid JSON." },
            2,
          );
        }
        data = await call(root, plugin!, method!, input);
        break;
      }
      case "dev":
        if (jsonMode)
          usage("dev --json is unsupported; use finite commands for structured results.");
        await dev({ root, entry });
        data = { stopped: true };
        break;
      case "help":
        data = help;
        break;
    }
    stableJson(data);
    const safe =
      command === "inspect"
        ? (() => {
            const { operations, ...metadata } = data as Awaited<ReturnType<typeof inspect>>;
            return { ...(redact(metadata, secrets) as typeof metadata), operations };
          })()
        : redact(data, secrets);
    stdout(
      `${stableJson(jsonMode ? { schemaVersion: 1, ok: true, data: safe } : safe, jsonMode ? undefined : 2)}\n`,
    );
  } catch (error) {
    span.setStatus({ code: SpanStatusCode.ERROR });
    // Remove absent optional diagnostic fields, preserving only JSON-safe public data.
    const detail = redact(JSON.parse(JSON.stringify(diagnostic(error))), secrets);
    if (jsonMode) stdout(`${stableJson({ schemaVersion: 1, ok: false, error: detail })}\n`);
    else process.stderr.write(`${stableJson(detail, 2)}\n`);
    process.exitCode = exitCode(error);
  } finally {
    span.end();
    await finishTelemetry();
  }
});
// An explicit SDK preload owns this callback; the CLI itself imports API only.
async function finishTelemetry(): Promise<void> {
  const finish = (
    globalThis as typeof globalThis & {
      [key: symbol]: (() => Promise<void>) | undefined;
    }
  )[Symbol.for("lenso.telemetry.cli.finish.v1")];
  if (finish) {
    try {
      await finish();
    } catch {
      process.stderr.write("Telemetry flush failed.\n");
    }
  }
}
