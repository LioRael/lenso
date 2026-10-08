import { serveStdio } from "@lenso/mcp";

await serveStdio({
  root: import.meta.dir,
  allow: (process.env.MCP_TEST_RUNTIME_ONLY === "1"
    ? ["echo"]
    : ["echo", "slow", "denied", "failed", "unsupported", "big"]
  ).map((method) => ({ pluginId: "fixture", method })),
  maxInputBytes: 256,
  maxOutputBytes: 512,
  maxFrameBytes: 4096,
}).catch(() => {
  process.stderr.write("adapter startup rejected\n");
  process.exitCode = 1;
});
