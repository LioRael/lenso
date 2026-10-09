import type { SearchConfig, SearchDocument, SearchReference, SearchScope } from "./contracts";
import { SearchError } from "./errors";

export function object(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype;
}

export function keys(value: Record<string, unknown>, allowed: readonly string[]): void {
  if (Object.keys(value).some((key) => !allowed.includes(key)))
    throw new SearchError("invalid-input");
}

function forbiddenControl(value: string): boolean {
  for (let i = 0; i < value.length; i++) {
    const code = value.charCodeAt(i);
    if (code < 32 && code !== 9 && code !== 10 && code !== 13) return true;
  }
  return false;
}

export function text(value: unknown, max: number, empty = false): string {
  if (
    typeof value !== "string" ||
    (!empty && !value.trim()) ||
    value.length > max ||
    forbiddenControl(value)
  )
    throw new SearchError("invalid-input");
  return value;
}

export function integer(value: unknown, min: number, max: number): number {
  if (!Number.isSafeInteger(value) || (value as number) < min || (value as number) > max)
    throw new SearchError("invalid-input");
  return value as number;
}

export function validateScope(value: unknown): SearchScope {
  if (!object(value) || value.namespace === undefined) throw new SearchError("missing-scope");
  keys(value, ["namespace", "tenantId", "ownerId"]);
  return Object.freeze({
    namespace: text(value.namespace, 128),
    ...(value.tenantId !== undefined ? { tenantId: text(value.tenantId, 128) } : {}),
    ...(value.ownerId !== undefined ? { ownerId: text(value.ownerId, 512) } : {}),
  });
}

export function scopeValues(scope: SearchScope): readonly [string, string, string] {
  return [scope.namespace, scope.tenantId ?? "", scope.ownerId ?? ""];
}

export function reference(value: unknown): SearchReference {
  if (!object(value)) throw new SearchError("invalid-input");
  keys(value, ["id", "type"]);
  return { id: text(value.id, 256), type: text(value.type, 64) };
}

export function validateKey(scope: SearchScope, ref: SearchReference): void {
  // Bound the composite B-tree key below PostgreSQL's default 8 KiB page limit.
  if (
    scopeValues(scope)
      .concat([ref.type, ref.id])
      .reduce((bytes, part) => bytes + Buffer.byteLength(part), 0) > 2400
  )
    throw new SearchError("invalid-input");
}

export function resolveSearchConfig(value: SearchConfig = {}): Readonly<Required<SearchConfig>> {
  if (!object(value)) throw new SearchError("invalid-input");
  const defaults = {
    enabled: true,
    maxPageSize: 50,
    maxOffset: 10_000,
    maxQueryChars: 512,
    maxTitleChars: 1024,
    maxBodyChars: 100_000,
    maxDocumentBytes: 524_288,
    maxMetadataBytes: 4096,
    summaryChars: 240,
  };
  keys(value as Record<string, unknown>, Object.keys(defaults));
  const config = { ...defaults, ...value };
  if (typeof config.enabled !== "boolean") throw new SearchError("invalid-input");
  integer(config.maxPageSize, 1, 100);
  integer(config.maxOffset, 0, 100_000);
  integer(config.maxQueryChars, 1, 4096);
  integer(config.maxTitleChars, 1, 4096);
  integer(config.maxBodyChars, 1, 1_000_000);
  integer(config.maxDocumentBytes, 1, 2_000_000);
  integer(config.maxMetadataBytes, 2, 16_384);
  integer(config.summaryChars, 1, 1000);
  return Object.freeze(config);
}

export function document(
  scope: SearchScope,
  value: unknown,
  config: Readonly<Required<SearchConfig>>,
): SearchDocument {
  if (!object(value)) throw new SearchError("invalid-input");
  keys(value, ["id", "type", "tenantId", "ownerId", "title", "body", "metadata"]);
  const ref = reference({ id: value.id, type: value.type });
  validateKey(scope, ref);
  for (const field of ["tenantId", "ownerId"] as const) {
    if (value[field] !== undefined) text(value[field], field === "ownerId" ? 512 : 128);
    if (value[field] !== scope[field]) throw new SearchError("scope-conflict");
  }
  const metadata = value.metadata ?? {};
  if (!object(metadata) || Object.keys(metadata).length > 16)
    throw new SearchError("invalid-input");
  const entries = Object.entries(metadata).map(([key, entry]) => {
    if (!/^[a-zA-Z][a-zA-Z0-9_]{0,63}$/.test(key)) throw new SearchError("invalid-input");
    if (typeof entry === "string") text(entry, 512, true);
    else if (
      entry !== null &&
      typeof entry !== "boolean" &&
      !(typeof entry === "number" && Number.isFinite(entry))
    )
      throw new SearchError("invalid-input");
    return [key, entry] as const;
  });
  const clean = {
    ...ref,
    ...(scope.tenantId !== undefined ? { tenantId: scope.tenantId } : {}),
    ...(scope.ownerId !== undefined ? { ownerId: scope.ownerId } : {}),
    title: text(value.title, config.maxTitleChars, true),
    body: text(value.body, config.maxBodyChars, true),
    metadata: Object.fromEntries(entries),
  };
  if (
    Buffer.byteLength(JSON.stringify(clean.metadata)) > config.maxMetadataBytes ||
    Buffer.byteLength(JSON.stringify(clean)) > config.maxDocumentBytes
  )
    throw new SearchError("invalid-input");
  return clean as SearchDocument;
}
