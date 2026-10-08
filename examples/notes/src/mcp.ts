import { serveStdio } from "@lenso/mcp";
import { fileURLToPath } from "node:url";

await serveStdio({
  root: fileURLToPath(new URL("../", import.meta.url)),
  allow: [
    { pluginId: "notes-operations", method: "list" },
    { pluginId: "notes-operations", method: "read" },
    { pluginId: "notes-operations", method: "remove" },
  ],
  binding: () => ({ context: { evidence: process.env.NOTES_MCP_SESSION ?? null } }),
}).catch(() => {
  process.stderr.write("Notes MCP startup failed; check trusted application configuration.\n");
  process.exitCode = 1;
});
