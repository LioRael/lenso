import { CacheError, type JsonValue } from "./contracts";

const encoder = new TextEncoder();

/** Reject JSON.stringify's lossy conversions, accessors and custom toJSON. */
function check(
  value: unknown,
  ancestors = new Set<object>(),
  depth = 0,
): asserts value is JsonValue {
  if (value === null || typeof value === "boolean" || typeof value === "string") return;
  if (typeof value === "number" && Number.isFinite(value)) return;
  if (typeof value !== "object" || value === null || depth >= 64 || ancestors.has(value))
    throw new CacheError("serialization");
  const array = Array.isArray(value);
  if (array && Object.getPrototypeOf(value) !== Array.prototype)
    throw new CacheError("serialization");
  if (
    !array &&
    Object.getPrototypeOf(value) !== Object.prototype &&
    Object.getPrototypeOf(value) !== null
  )
    throw new CacheError("serialization");
  ancestors.add(value);
  try {
    let count = 0;
    for (const key of Reflect.ownKeys(value)) {
      if (array && key === "length") continue;
      if (typeof key !== "string") throw new CacheError("serialization");
      const descriptor = Object.getOwnPropertyDescriptor(value, key)!;
      if (!descriptor.enumerable || !("value" in descriptor)) throw new CacheError("serialization");
      if (array && (!/^(0|[1-9]\d*)$/.test(key) || Number(key) >= (value as unknown[]).length))
        throw new CacheError("serialization");
      check(descriptor.value, ancestors, depth + 1);
      count++;
    }
    if (array && count !== (value as unknown[]).length) throw new CacheError("serialization");
  } finally {
    ancestors.delete(value);
  }
}

export function encode(
  value: JsonValue,
  expiresAt: number,
  maxBytes: number,
  validate?: (value: JsonValue) => boolean,
): string {
  try {
    check(value);
    if (validate && !validate(value)) throw new CacheError("serialization");
    const text = JSON.stringify({ v: 1, expiresAt, value });
    if (encoder.encode(text).length > maxBytes) throw new CacheError("serialization");
    return text;
  } catch {
    throw new CacheError("serialization");
  }
}

export function decode<T extends JsonValue>(
  text: string,
  maxBytes: number,
  validate?: (value: JsonValue) => value is T,
): { expiresAt: number; value: T } {
  if (encoder.encode(text).length > maxBytes) throw new CacheError("serialization");
  const envelope = JSON.parse(text);
  if (
    envelope === null ||
    typeof envelope !== "object" ||
    envelope.v !== 1 ||
    !Number.isSafeInteger(envelope.expiresAt) ||
    envelope.expiresAt < 0 ||
    !Object.hasOwn(envelope, "value") ||
    Object.keys(envelope).length !== 3
  )
    throw new CacheError("serialization");
  check(envelope.value);
  if (validate && !validate(envelope.value)) throw new CacheError("serialization");
  return { expiresAt: envelope.expiresAt, value: envelope.value as T };
}
