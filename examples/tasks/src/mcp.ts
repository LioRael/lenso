import { serveStdio } from "@lenso/mcp";
import { fileURLToPath } from "node:url";

await serveStdio({
  root: fileURLToPath(new URL("../", import.meta.url)),
  allow: [
    { pluginId: "tasks", method: "submit" },
    { pluginId: "tasks", method: "query" },
    { pluginId: "tasks", method: "cancel" },
    { pluginId: "tasks", method: "retry" },
  ],
}).catch(() => {
  process.stderr.write("Tasks MCP startup failed; check trusted application configuration.\n");
  process.exitCode = 1;
});
