import { serveStdio } from "@lenso/mcp";
import { fileURLToPath } from "node:url";

await serveStdio({
  root: fileURLToPath(new URL("../", import.meta.url)),
  allow: [
    { pluginId: "notes-operations", method: "create" },
    { pluginId: "notes-operations", method: "list" },
    { pluginId: "notes-operations", method: "read" },
    { pluginId: "notes-operations", method: "update" },
    { pluginId: "notes-operations", method: "remove" },
  ],
}).catch(() => {
  process.stderr.write("Notes MCP startup failed; check trusted application configuration.\n");
  process.exitCode = 1;
});
