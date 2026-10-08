import type { ConfigPath, Plugin } from "@lenso/core";
import { redact, stableJson } from "./diagnostics";
import { safeInputSchema } from "./operations";

/** Static metadata only. No reads, revisions, resolved values, or inferred output provenance. */
export function describePluginConfig(plugin: Plugin<unknown>, configPath: string) {
  if (!plugin.config) return null;
  const { contract, sources } = plugin.config;
  const sensitive = [
    ...(contract.fields ?? []).filter((field) => field.sensitive).map((field) => field.path),
    ...sources.flatMap((source) =>
      (source.descriptor.fields ?? [])
        .filter((field) => field.sensitive)
        .map((field) => field.path),
    ),
  ];
  let inputSchema: Record<string, unknown> | null = null;
  if (contract.jsonSchema) {
    try {
      // Round trip rejects non-plain data and prevents mutation of the converter's object.
      const schema = JSON.parse(stableJson(contract.jsonSchema())) as Record<string, unknown>;
      for (const path of sensitive) hideSensitiveSchema(schema, path);
      inputSchema = safeInputSchema(sensitive.length ? shapeOnlySchema(schema) : schema, []);
    } catch {
      // A converter is optional trusted code, not a promise of static representability.
    }
  }
  return {
    schemaAvailability: inputSchema ? ("available" as const) : ("runtime-validation-only" as const),
    description: safeText(contract.description),
    fields: (contract.fields ?? []).map((field) => ({
      path: safePath(field.path),
      description: safeText(field.description),
      sensitive: field.sensitive ?? false,
    })),
    sources: sources.map(({ descriptor }) => ({
      id: safeText(descriptor.id),
      kind: safeText(descriptor.kind),
      location: redact(descriptor.location ?? { file: configPath }),
      fields: (descriptor.fields ?? []).map((field) => ({
        path: safePath(field.path),
        ...(field.env === undefined ? {} : { env: safeText(field.env) }),
        sensitive: field.sensitive ?? false,
      })),
    })),
    inputSchema,
  };
}

function safeText(value: string | undefined): string | null {
  return value === undefined ? null : (redact(value) as string);
}

function safePath(path: ConfigPath): ConfigPath {
  return path.map((segment) =>
    typeof segment === "string" ? (redact(segment) as string) : segment,
  );
}

function hideSensitiveSchema(schema: Record<string, unknown>, path: ConfigPath): void {
  if (!path.length) {
    for (const key of Object.keys(schema)) delete schema[key];
    schema.writeOnly = true;
    return;
  }
  // References/combinators can carry the same secret annotations elsewhere. When a
  // sensitive input exists, omit them rather than claiming precise reference tracking.
  for (const key of ["$defs", "definitions", "allOf", "anyOf", "oneOf", "if", "then", "else"])
    delete schema[key];
  const properties = schema.properties;
  if (!properties || typeof properties !== "object" || Array.isArray(properties)) return;
  const field = String(path[0]);
  if (!Object.hasOwn(properties, field)) return;
  const child = Reflect.get(properties, field);
  if (child && typeof child === "object" && !Array.isArray(child)) {
    // Hide the entire top-level subtree, including array items and nested references.
    Reflect.set(properties, field, { writeOnly: true });
  }
}

function shapeOnlySchema(schema: Record<string, unknown>): Record<string, unknown> {
  // Constraints and extensions can repeat values outside their property's subtree
  // (const objects, dependentSchemas, patterns, refs). Show only shape when secrets exist.
  const result: Record<string, unknown> = {};
  const types = new Set(["object", "array", "string", "number", "integer", "boolean", "null"]);
  if (typeof schema.type === "string" && types.has(schema.type)) result.type = schema.type;
  if (Array.isArray(schema.type) && schema.type.every((type) => types.has(type)))
    result.type = schema.type;
  if (schema.writeOnly === true) result.writeOnly = true;
  if (Array.isArray(schema.required) && schema.required.every((key) => typeof key === "string"))
    result.required = schema.required;
  for (const key of ["items", "additionalProperties"]) {
    const value = schema[key];
    if (typeof value === "boolean") result[key] = value;
    else if (value && typeof value === "object" && !Array.isArray(value))
      result[key] = shapeOnlySchema(value as Record<string, unknown>);
  }
  const properties = schema.properties;
  if (properties && typeof properties === "object" && !Array.isArray(properties))
    result.properties = Object.fromEntries(
      Object.entries(properties).map(([key, value]) => [
        key,
        typeof value === "boolean"
          ? value
          : value && typeof value === "object" && !Array.isArray(value)
            ? shapeOnlySchema(value as Record<string, unknown>)
            : {},
      ]),
    );
  return result;
}
