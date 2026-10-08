import { CacheError, type CacheConfig } from "./contracts";

export const MAX_TTL_MS = 86_400_000;

export function validateName(value: string): void {
  if (
    typeof value !== "string" ||
    !value ||
    new TextEncoder().encode(value).length > 256 ||
    [...value].some((part) => {
      const code = part.charCodeAt(0);
      return (
        code < 32 ||
        (code >= 127 && code <= 159) ||
        (part.length === 1 && code >= 0xd800 && code <= 0xdfff)
      );
    })
  ) {
    throw new CacheError("invalid-input");
  }
}

export function validateTtl(ttl: number, max: number): void {
  if (!Number.isSafeInteger(ttl) || ttl < 0 || ttl > max) throw new CacheError("invalid-input");
}

export function resolveCacheConfig(config: CacheConfig): Readonly<Required<CacheConfig>> {
  validateName(config.namespace);
  for (const key of ["defaultTtlMs", "maxTtlMs", "maxValueBytes", "maxInFlight"] as const) {
    if (config[key] !== undefined && typeof config[key] !== "number")
      throw new CacheError("invalid-input");
  }
  if (
    config.failureMode !== undefined &&
    config.failureMode !== "fail-open" &&
    config.failureMode !== "fail-closed"
  )
    throw new CacheError("invalid-input");
  const resolved = {
    namespace: config.namespace,
    defaultTtlMs: config.defaultTtlMs ?? 60_000,
    maxTtlMs: config.maxTtlMs ?? MAX_TTL_MS,
    maxValueBytes: config.maxValueBytes ?? 65_536,
    maxInFlight: config.maxInFlight ?? 128,
    failureMode: config.failureMode ?? "fail-closed",
  } as const;
  validateTtl(resolved.maxTtlMs, MAX_TTL_MS);
  if (resolved.maxTtlMs === 0) throw new CacheError("invalid-input");
  validateTtl(resolved.defaultTtlMs, resolved.maxTtlMs);
  for (const [value, max] of [
    [resolved.maxValueBytes, 1_048_576],
    [resolved.maxInFlight, 10_000],
  ]) {
    if (!Number.isSafeInteger(value) || value < 1 || value > max)
      throw new CacheError("invalid-input");
  }
  if (resolved.failureMode !== "fail-open" && resolved.failureMode !== "fail-closed")
    throw new CacheError("invalid-input");
  return Object.freeze(resolved);
}
