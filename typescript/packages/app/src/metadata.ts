/** Optional public JSON annotations, never a replacement for native parsers or policies. */
export type JsonSchema = boolean | Readonly<Record<string, unknown>>;

function object(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function schemaValue(value: unknown): value is JsonSchema {
  return typeof value === "boolean" || object(value);
}

export interface ActionMetadata {
  readonly description?: string | undefined;
  readonly inputSchema?: JsonSchema | undefined;
  readonly outputSchema?: JsonSchema | undefined;
  readonly capabilities?: readonly string[] | undefined;
  readonly effects?: readonly string[] | undefined;
}

export interface ModuleMetadata {
  readonly description?: string | undefined;
  readonly version?: string | undefined;
  readonly capabilities?: readonly string[] | undefined;
  readonly effects?: readonly string[] | undefined;
}

export interface ActionDescription {
  readonly id: string;
  /** Null means undeclared, including for legacy Actions. Empty effects/capabilities declare none. */
  readonly metadata: ActionMetadata | null;
}

/** Reads only static declarations; it never parses input, authorizes, or runs the Action. */
export function describeAction(action: { readonly id: string; readonly metadata?: ActionMetadata }): ActionDescription {
  return { id: action.id, metadata: action.metadata === undefined ? null : structuredClone(action.metadata) };
}

/** Keeps only paths named by inline public schema properties/items; dynamic keys stay private. */
export function publicInputPath(path: readonly (string | number)[], schema?: JsonSchema): (string | number)[] {
  const result: (string | number)[] = [];
  for (const segment of path) {
    if (typeof schema !== "object") break;
    if (typeof segment === "number" && Number.isSafeInteger(segment) && segment >= 0) {
      const items = schema.items;
      if (!schemaValue(items)) break;
      schema = items;
    } else {
      const properties = schema.properties;
      if (typeof segment !== "string" || !object(properties)
        || !Object.hasOwn(properties, segment)) break;
      const child = properties[segment];
      if (!schemaValue(child)) break;
      schema = child;
    }
    result.push(segment);
  }
  return result;
}
