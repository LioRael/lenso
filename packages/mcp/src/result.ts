import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";

/** Project only the already redacted, finite, budget-checked JSON sent as text. */
export function diagnosticResult(text: string, structured = true): CallToolResult {
  const detail = JSON.parse(text) as Record<string, unknown>;
  return {
    isError: true,
    content: [{ type: "text", text }],
    _meta: { "lenso/diagnostic": detail },
    // SDK clients validate any structuredContent against the successful output
    // contract, even for isError. Keep that contract intact when declared.
    ...(structured ? { structuredContent: detail } : {}),
  };
}
