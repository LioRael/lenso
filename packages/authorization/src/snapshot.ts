/** Copy finite plain facts before awaiting user code; never freeze borrowed application objects. */
export function snapshot<T>(value: T, limits = { maxNodes: 10_000, maxDepth: 64 }): T {
  let nodes = 0;
  const visiting = new Set<object>();
  function copy(item: unknown, depth: number): unknown {
    if (++nodes > limits.maxNodes || depth > limits.maxDepth) throw new Error("Invalid facts");
    if (
      item === null ||
      item === undefined ||
      typeof item === "string" ||
      typeof item === "boolean"
    )
      return item;
    if (typeof item === "number" && Number.isFinite(item)) return item;
    if (typeof item !== "object" || visiting.has(item)) throw new Error("Invalid facts");
    if (!Array.isArray(item) && ![Object.prototype, null].includes(Object.getPrototypeOf(item)))
      throw new Error("Invalid facts");
    visiting.add(item);
    const result: Record<string, unknown> | unknown[] = Array.isArray(item) ? [] : {};
    for (const key of Reflect.ownKeys(item)) {
      if (Array.isArray(item) && key === "length") continue;
      const property = Object.getOwnPropertyDescriptor(item, key)!;
      if (typeof key !== "string" || !property.enumerable || !("value" in property))
        throw new Error("Invalid facts");
      Object.defineProperty(result, key, {
        value: copy(property.value, depth + 1),
        enumerable: true,
      });
    }
    visiting.delete(item);
    return Object.freeze(result);
  }
  return copy(value, 0) as T;
}
