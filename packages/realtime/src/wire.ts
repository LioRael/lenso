import { RealtimeError, type Cursor, type Json, type Resource } from "./contracts";

export function part(value: string): string {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.length > 256 ||
    new TextEncoder().encode(value).byteLength > 256
  )
    throw new RealtimeError("invalid-input");
  try {
    return encodeURIComponent(value);
  } catch {
    throw new RealtimeError("invalid-input");
  }
}

export function resourceTopic(resource: Resource): string {
  return `resource/${part(resource.scope)}/${part(resource.type)}/${part(resource.id)}`;
}

export function cursorToken(cursor: Cursor, now = Date.now()): string {
  return `${cursor.generation}.${cursor.sequence}.${now}`;
}

export function parseCursor(token: string): (Cursor & { issuedAt: number }) | undefined {
  if (typeof token !== "string" || token.length > 160) return;
  const match = /^([A-Za-z0-9-]{1,64})\.(\d{1,16})\.(\d{1,16})$/.exec(token);
  if (!match) return;
  const sequence = Number(match[2]);
  const issuedAt = Number(match[3]);
  if (!Number.isSafeInteger(sequence) || !Number.isSafeInteger(issuedAt)) return;
  return { generation: match[1], sequence, issuedAt };
}

export function sameCursor(a: Cursor, b: Cursor): boolean {
  return a.generation === b.generation && a.sequence === b.sequence;
}

/** No toJSON, accessors, classes, sparse arrays, cycles, non-finite numbers or lossy values. */
export function serialize(value: unknown, maxBytes: number): string {
  const active = new Set<object>();
  let nodes = 0;
  const visit = (input: unknown, depth: number): void => {
    if (++nodes > 10000 || depth > 32) throw new RealtimeError("payload");
    if (input === null || typeof input === "string" || typeof input === "boolean") return;
    if (typeof input === "number" && Number.isFinite(input)) return;
    if (typeof input !== "object" || input === null || active.has(input))
      throw new RealtimeError("payload");
    const proto = Object.getPrototypeOf(input);
    if (
      Array.isArray(input)
        ? proto !== Array.prototype
        : proto !== Object.prototype && proto !== null
    )
      throw new RealtimeError("payload");
    active.add(input);
    if (Object.getOwnPropertySymbols(input).length) throw new RealtimeError("payload");
    const keys = Object.keys(input);
    const ownNames = Object.getOwnPropertyNames(input);
    if (ownNames.length !== keys.length + (Array.isArray(input) ? 1 : 0))
      throw new RealtimeError("payload");
    if (
      Array.isArray(input) &&
      (keys.length !== input.length || keys.some((key, i) => key !== String(i)))
    )
      throw new RealtimeError("payload");
    for (const key of keys) {
      const descriptor = Object.getOwnPropertyDescriptor(input, key)!;
      if (!("value" in descriptor)) throw new RealtimeError("payload");
      visit(descriptor.value, depth + 1);
    }
    active.delete(input);
  };
  visit(value, 0);
  const text = JSON.stringify(value as Json);
  if (new TextEncoder().encode(text).byteLength > maxBytes) throw new RealtimeError("payload");
  return text;
}
