import { createHmac, timingSafeEqual } from "node:crypto";
import type {
  SearchConfig,
  SearchProvider,
  SearchQuery,
  SearchScope,
  SearchService,
} from "./contracts";
import { SearchError } from "./errors";
import {
  document,
  integer,
  keys,
  object,
  reference,
  resolveSearchConfig,
  scopeValues,
  text,
  validateKey,
  validateScope,
} from "./validation";

export type * from "./contracts";
export { SearchError, searchErrorDiagnostic } from "./errors";
export { resolveSearchConfig } from "./validation";

export function createSearchService(options: {
  readonly provider: SearchProvider;
  /** Host-supplied random secret, at least 32 bytes; rotation invalidates cursors. */
  readonly cursorSecret: Uint8Array;
  readonly config?: SearchConfig;
}): SearchService {
  const config = resolveSearchConfig(options.config);
  if (!(options.cursorSecret instanceof Uint8Array) || options.cursorSecret.byteLength < 32)
    throw new SearchError("invalid-input");
  const secret = Buffer.from(options.cursorSecret);
  const provider = options.provider;
  function scope(value: SearchScope) {
    const validated = validateScope(value);
    if (!config.enabled) throw new SearchError("disabled");
    return validated;
  }
  function mac(value: string): Buffer {
    return createHmac("sha256", secret).update(value).digest();
  }
  async function invoke<T>(write: boolean, run: () => Promise<T>): Promise<T> {
    try {
      return await run();
    } catch (error) {
      if (error instanceof SearchError) throw error;
      throw new SearchError(write ? "index-failed" : "query-failed");
    }
  }
  return {
    capabilities: provider.capabilities,
    async upsert(value, input) {
      const authorized = scope(value);
      const clean = document(authorized, input, config);
      await invoke(true, () => provider.upsert(authorized, clean));
    },
    async delete(value, input) {
      const authorized = scope(value);
      const clean = reference(input);
      validateKey(authorized, clean);
      await invoke(true, () => provider.delete(authorized, clean));
    },
    async query(value, input: SearchQuery) {
      const authorized = scope(value);
      if (!object(input)) throw new SearchError("invalid-input");
      keys(input, ["text", "type", "pageSize", "cursor", "sort", "includeTotal"]);
      const queryText = text(input.text, config.maxQueryChars, true).trim();
      const type = input.type === undefined ? undefined : text(input.type, 64);
      const pageSize = integer(
        input.pageSize ?? Math.min(20, config.maxPageSize),
        1,
        config.maxPageSize,
      );
      const sort = input.sort ?? "relevance";
      if (sort !== "relevance" && sort !== "id") throw new SearchError("unsupported-capability");
      if (!provider.capabilities.fullText || !provider.capabilities.sorts.includes(sort))
        throw new SearchError("unsupported-capability");
      const includeTotal = input.includeTotal ?? false;
      if (typeof includeTotal !== "boolean") throw new SearchError("invalid-input");
      if (includeTotal && provider.capabilities.count !== "exact")
        throw new SearchError("unsupported-capability");
      const binding = mac(
        JSON.stringify([
          "search-offset-v1",
          provider.identity,
          config,
          scopeValues(authorized),
          queryText,
          type ?? null,
          pageSize,
          sort,
          includeTotal,
        ]),
      ).toString("base64url");
      let offset = 0;
      if (input.cursor !== undefined) {
        try {
          if (typeof input.cursor !== "string" || input.cursor.length > 512) throw new Error();
          const [payload, signature, extra] = input.cursor.split(".");
          if (!payload || !signature || extra !== undefined) throw new Error();
          const supplied = Buffer.from(signature, "base64url");
          const expected = mac(payload);
          if (supplied.length !== expected.length || !timingSafeEqual(supplied, expected))
            throw new Error();
          const parsed = JSON.parse(Buffer.from(payload, "base64url").toString("utf8"));
          if (!object(parsed) || parsed.binding !== binding) throw new Error();
          keys(parsed, ["binding", "offset"]);
          offset = integer(parsed.offset, 1, config.maxOffset);
          if (offset % pageSize !== 0) throw new Error();
        } catch {
          throw new SearchError("invalid-cursor");
        }
      }
      if (!queryText) return { hits: [], ...(includeTotal ? { total: 0 } : {}) };
      const page = await invoke(false, () =>
        provider.query(authorized, {
          text: queryText,
          type,
          pageSize,
          sort,
          includeTotal,
          offset,
          summaryChars: config.summaryChars,
        }),
      );
      const nextOffset = offset + pageSize;
      let nextCursor: string | undefined;
      if (page.hasMore && nextOffset <= config.maxOffset) {
        const payload = Buffer.from(JSON.stringify({ binding, offset: nextOffset })).toString(
          "base64url",
        );
        nextCursor = `${payload}.${mac(payload).toString("base64url")}`;
      }
      return {
        hits: page.hits,
        ...(nextCursor ? { nextCursor } : {}),
        ...(includeTotal ? { total: page.total } : {}),
      };
    },
  };
}
