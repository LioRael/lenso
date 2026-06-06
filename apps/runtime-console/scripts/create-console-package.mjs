import { fileURLToPath } from "node:url";

import { runConsolePackageCli } from "@lenso/console-package-cli";

const runtimeConsoleRoot = fileURLToPath(new URL("..", import.meta.url));
const exitCode = await runConsolePackageCli(
  ["create", ...process.argv.slice(2)],
  { defaultRuntimeConsoleRoot: runtimeConsoleRoot }
);

process.exit(exitCode);
