import { definePlugin } from "@lenso/bun-plugin";
import { tool, tools } from "@lenso/agent-tool-sdk";
import * as schema from "@lenso/agent-tool-sdk/schema";

export default definePlugin({
  providers: [
    tools([
      tool(
        {
          name: "example.bun-a",
          description: "Return a string through a real Capability call.",
          input: schema.object({ text: schema.string() }),
          output: schema.string(),
          execution: "parallel_safe",
        },
        () => ({ ok: true, value: "bun-baseline" }),
      ),
    ]),
  ],
});
