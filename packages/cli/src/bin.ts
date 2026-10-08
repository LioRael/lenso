#!/usr/bin/env bun
import { fileURLToPath } from "node:url";
import { build, call, discover, generate } from "./engine";
import { dev } from "./dev";

const args = process.argv.slice(2);
const command = args.shift() ?? "help";
function option(name: string): string | undefined {
  const index = args.indexOf(name);
  if (index < 0) return undefined;
  const value = args[index + 1];
  if (!value || value.startsWith("--")) throw new Error(`Missing value for ${name}`);
  args.splice(index, 2);
  return value;
}

try {
  const root = option("--root") ?? process.cwd();
  const entry = option("--entry");
  switch (command) {
    case "check": {
      const discovery = await discover(root);
      console.log(
        JSON.stringify(
          { valid: true, order: discovery.ordered.map((plugin) => plugin.id) },
          null,
          2,
        ),
      );
      break;
    }
    case "generate": {
      const manifest = await generate(root);
      console.log(`[lenso] Generated ${manifest.length} plugin(s) in .lenso`);
      break;
    }
    case "build":
      console.log(`[lenso] Built ${await build(root, entry)}`);
      break;
    case "call": {
      const [plugin, method, json = "{}"] = args;
      if (!plugin || !method)
        throw new Error("Usage: lenso call <plugin-id> <method> [JSON input]");
      console.log(JSON.stringify(await call(root, plugin, method, JSON.parse(json)), null, 2));
      break;
    }
    case "dev":
      await dev({ root, entry, cliPath: fileURLToPath(import.meta.url) });
      break;
    case "help":
      console.log(
        "lenso <check|generate|build|call|dev> [--root directory] [--entry src/server.ts]\ncall: lenso call <plugin-id> <method> [JSON input]",
      );
      break;
    default:
      throw new Error(`Unknown command: ${command}`);
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
