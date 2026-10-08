import type { ManageAdapter } from "./adapter";
import { EngineError, stableJson } from "@lenso/engine/diagnostics";

export interface AgentTool {
  readonly name: string;
  readonly description: string;
  readonly inputSchema: Record<string, unknown>;
  invoke(input: unknown): Promise<unknown>;
}

export async function createAgentTools(adapter: ManageAdapter): Promise<readonly AgentTool[]> {
  const catalog = await adapter.catalog();
  return Object.freeze(
    catalog.map((operation) => {
      const schema = operation.inputSchema;
      if (!schema || schema.type !== "object")
        throw new EngineError({
          code: "unsupported-input-schema",
          phase: "discovery",
          message: "Agent tools require a convertible object input schema.",
        });
      const inputSchema = JSON.parse(stableJson(schema)) as Record<string, unknown>;
      return Object.freeze({
        name: operation.key,
        description: operation.description,
        inputSchema,
        invoke: (input: unknown) => adapter.invokeEntry(operation.key, input),
      });
    }),
  );
}
