/** Owns a native resource; acquisition failure must clean up its partial effects. */
export interface Resource<T> {
  open(): T | Promise<T>;
  close(value: T): void | Promise<void>;
}

export interface ResourceDefinition<C, T> {
  readonly id: string;
  readonly config: Parser<C>;
  open(config: C): T | Promise<T>;
  close(value: T): void | Promise<void>;
}

/** Binds validated configuration without opening a connection or wrapping its native value. */
export function defineResource<C, T>(definition: ResourceDefinition<C, T>) {
  return {
    id: definition.id,
    configure(config: unknown): Resource<T> {
      return {
        open() {
          let parsed: C;
          try { parsed = definition.config.parse(config); }
          catch (cause) { throw new ActionError("invalid_input", `Invalid resource configuration: ${definition.id}`, { cause }); }
          return definition.open(parsed);
        },
        close: (value) => definition.close(value),
      };
    },
  };
}

type ResourceValues<T> = { readonly [K in keyof T]: T[K] extends Resource<infer V> ? V : never };

/** Opens explicit dependencies in order and unwinds every acquired resource in reverse order. */
export async function withResources<T extends Record<string, Resource<unknown>>, O>(
  resources: T,
  use: (values: ResourceValues<T>) => O | Promise<O>,
): Promise<O> {
  const entries = Object.entries(resources);
  const values: Record<string, unknown> = {};
  const open = async (index: number): Promise<O> => {
    const entry = entries[index];
    // Every key is populated from its corresponding resource before exposing the typed map.
    if (!entry) return use(values as ResourceValues<T>);
    const [key, resource] = entry;
    return withResource(resource, (value) => {
      values[key] = value;
      return open(index + 1);
    });
  };
  return open(0);
}

/** Nested calls close in reverse order; failures from both use and close are retained. */
export async function withResource<T, O>(
  resource: Resource<T>,
  use: (value: T) => O | Promise<O>,
): Promise<O> {
  const value = await resource.open();
  let result: O;
  try {
    result = await use(value);
  } catch (useError) {
    try {
      await resource.close(value);
    } catch (closeError) {
      throw new AggregateError([useError, closeError], "Resource use and cleanup failed");
    }
    throw useError;
  }
  await resource.close(value);
  return result;
}
import { ActionError } from "./error.js";
import type { Parser } from "./schema.js";
