import type { StandardSchemaV1 } from "@standard-schema/spec";
import { definePluginConfig } from "@lenso/core/config";
import { LimitError, type LimitConfig } from "./contracts";

export function validateConfig(value: unknown): LimitConfig {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    Object.keys(value).some((key) => key !== "failurePolicy") ||
    !("failurePolicy" in value) ||
    !["throw", "deny", "allow"].includes(value.failurePolicy as string)
  )
    throw new LimitError("invalid-input");
  return { failurePolicy: value.failurePolicy as LimitConfig["failurePolicy"] };
}

export const limitConfigSchema: StandardSchemaV1<LimitConfig, LimitConfig> = {
  "~standard": {
    version: 1,
    vendor: "@lenso/limits",
    validate(value) {
      try {
        return { value: validateConfig(value) };
      } catch {
        return { issues: [{ message: "Select an explicit limit backend failure policy." }] };
      }
    },
  },
};

export const limitConfig = definePluginConfig({
  schema: limitConfigSchema,
  description: "Explicit admission behavior on limit backend failure",
  fields: [{ path: ["failurePolicy"], description: "throw, deny, or visibly degraded allow" }],
  jsonSchema: () => ({
    type: "object",
    additionalProperties: false,
    required: ["failurePolicy"],
    properties: { failurePolicy: { type: "string", enum: ["throw", "deny", "allow"] } },
  }),
});
