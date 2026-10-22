import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { boundedJson, type OperationExecutionEvidence } from "@lenso/engine/operations";
import { environmentSecrets, redact } from "@lenso/engine/diagnostics";

export const maxExecutionEvidenceBytes = 1024;

/** Optional correlation must never replace a settled business result with an output failure. */
export function executionResult(
  result: CallToolResult,
  evidence: OperationExecutionEvidence | undefined,
  maxBytes: number,
): CallToolResult {
  if (!evidence) return result;
  try {
    const safe = JSON.parse(
      boundedJson(
        redact(evidence, environmentSecrets()),
        Math.min(maxBytes, maxExecutionEvidenceBytes),
      ),
    );
    if (
      safe.trace &&
      (!/^[0-9a-f]{32}$/.test(safe.trace.traceId) ||
        /^0+$/.test(safe.trace.traceId) ||
        !/^[0-9a-f]{16}$/.test(safe.trace.spanId) ||
        /^0+$/.test(safe.trace.spanId))
    )
      safe.trace = null;
    const { _meta: metadata } = result;
    return { ...result, _meta: { ...metadata, "lenso/execution": safe } };
  } catch {
    // Omit optional evidence if redaction/serialization/host budget cannot represent it.
    return result;
  }
}

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
