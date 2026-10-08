import { types } from "node:util";
import type { JsonValue } from "./contracts";
import { TaskQueueError } from "./errors";

export const INPUT_LIMIT_BYTES = 64 * 1024;
export const RESULT_LIMIT_BYTES = 16 * 1024;

/** Validate before stringify: JSON.stringify silently drops or coerces unsupported values. */
export function copyJson(
  value: unknown,
  limit: number,
  code: "invalid-input" | "invalid-result",
): JsonValue {
  const ancestors = new Set<object>();
  let nodes = 0;
  let stringBytes = 0;
  function reject(): never {
    throw new TaskQueueError(code);
  }
  function visit(item: unknown, depth: number): JsonValue {
    if (depth > 32 || ++nodes > limit) reject();
    if (item === null || typeof item === "boolean") return item;
    if (typeof item === "string") {
      stringBytes += new TextEncoder().encode(item).byteLength;
      if (stringBytes > limit) reject();
      return item;
    }
    if (typeof item === "number") {
      if (!Number.isFinite(item)) reject();
      return item;
    }
    if (typeof item !== "object" || types.isProxy(item) || ancestors.has(item)) reject();
    const array = Array.isArray(item);
    const prototype = Object.getPrototypeOf(item);
    if (
      array ? prototype !== Array.prototype : prototype !== Object.prototype && prototype !== null
    )
      reject();
    ancestors.add(item);
    const keys = Reflect.ownKeys(item);
    if (array && keys.length !== item.length + 1) reject();
    const snapshot: JsonValue[] | { [key: string]: JsonValue } = array ? [] : Object.create(null);
    for (const key of keys) {
      if (array && key === "length") continue;
      if (typeof key !== "string") reject();
      if (array && (!/^(0|[1-9]\d*)$/.test(key) || Number(key) >= item.length)) reject();
      const descriptor = Object.getOwnPropertyDescriptor(item, key)!;
      if (!descriptor.enumerable || !("value" in descriptor)) reject();
      if (!array) stringBytes += new TextEncoder().encode(key).byteLength;
      if (stringBytes > limit) reject();
      const child = visit(descriptor.value, depth + 1);
      if (array) (snapshot as JsonValue[])[Number(key)] = child;
      else (snapshot as { [key: string]: JsonValue })[key] = child;
    }
    ancestors.delete(item);
    return snapshot;
  }
  try {
    const snapshot = visit(value, 0);
    const encoded = JSON.stringify(snapshot);
    if (new TextEncoder().encode(encoded).byteLength > limit) reject();
    return JSON.parse(encoded) as JsonValue;
  } catch {
    throw new TaskQueueError(code);
  }
}
