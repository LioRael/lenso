import type { ProviderQuery, SearchConfig, SearchProvider } from "./contracts";
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

/** Host owns the client, connection configuration and transaction boundaries. */
export interface SearchDatabase {
  execute(
    text: string,
    parameters: readonly unknown[],
  ): Promise<readonly Record<string, unknown>[]>;
}

export interface PostgresSearchOptions {
  readonly database: SearchDatabase;
  readonly table?: string;
  readonly language?: "simple" | "english";
  readonly config?: SearchConfig;
}

export function searchTable(value = "lenso_search_documents"): string {
  if (typeof value !== "string" || !/^[a-z_][a-z0-9_]{0,47}$/.test(value))
    throw new SearchError("invalid-input");
  return `"${value}"`;
}

/** SQL is returned for review/execution by the host, never run during construction. */
export function postgresSearchMigration(table = "lenso_search_documents"): string {
  const name = searchTable(table);
  return `CREATE TABLE ${name} (
  namespace text NOT NULL,
  tenant_id text NOT NULL DEFAULT '',
  owner_id text NOT NULL DEFAULT '',
  document_type text COLLATE "C" NOT NULL,
  document_id text COLLATE "C" NOT NULL,
  title text NOT NULL,
  body text NOT NULL,
  metadata jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(metadata) = 'object'),
  language regconfig NOT NULL CHECK (language IN ('simple'::regconfig, 'english'::regconfig)),
  search_vector tsvector GENERATED ALWAYS AS (
    setweight(to_tsvector(language, title), 'A') ||
    setweight(to_tsvector(language, body), 'B')
  ) STORED,
  PRIMARY KEY (namespace, tenant_id, owner_id, document_type, document_id)
);
CREATE INDEX "${table}_fts" ON ${name} USING gin (search_vector);
CREATE INDEX "${table}_scope" ON ${name} (namespace, tenant_id, owner_id, language);
`;
}

export function createPostgresSearchProvider(options: PostgresSearchOptions): SearchProvider {
  const table = searchTable(options.table);
  const language = options.language ?? "simple";
  if (language !== "simple" && language !== "english") throw new SearchError("invalid-input");
  const config = resolveSearchConfig(options.config);
  async function execute(write: boolean, sql: string, parameters: readonly unknown[]) {
    if (!config.enabled) throw new SearchError("disabled");
    try {
      return await options.database.execute(sql, parameters);
    } catch (error) {
      // Driver diagnostics are deliberately not retained as Error.cause.
      const code =
        typeof error === "object" && error !== null && "code" in error ? String(error.code) : "";
      const sqlState =
        typeof error === "object" && error !== null && "errno" in error
          ? String(error.errno)
          : code;
      if (
        /^(08|53|57P)/.test(sqlState) ||
        [
          "ECONNREFUSED",
          "ECONNRESET",
          "ETIMEDOUT",
          "ERR_POSTGRES_CONNECTION_CLOSED",
          "ERR_POSTGRES_CONNECTION_TIMEOUT",
          "ERR_POSTGRES_CONNECTION_CANCELED",
          "ERR_POSTGRES_FAILED_TO_CONNECT",
        ].includes(code)
      )
        throw new SearchError("db-unavailable");
      throw new SearchError(write ? "index-failed" : "query-failed");
    }
  }
  return {
    identity: `postgres:${table}:${language}`,
    capabilities: Object.freeze({
      fullText: true,
      sorts: Object.freeze(["relevance", "id"] as const),
      pagination: "offset",
      summary: "plain-text",
      count: "exact",
    }),
    async upsert(value, input) {
      const scope = validateScope(value);
      const clean = document(scope, input, config);
      await execute(
        true,
        `INSERT INTO ${table}
        (namespace, tenant_id, owner_id, document_type, document_id, title, body, metadata, language)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8::text::jsonb, $9::regconfig)
        ON CONFLICT (namespace, tenant_id, owner_id, document_type, document_id)
        DO UPDATE SET title = EXCLUDED.title, body = EXCLUDED.body,
          metadata = EXCLUDED.metadata, language = EXCLUDED.language`,
        [
          ...scopeValues(scope),
          clean.type,
          clean.id,
          clean.title,
          clean.body,
          JSON.stringify(clean.metadata),
          language,
        ],
      );
    },
    async delete(value, input) {
      const scope = validateScope(value);
      const clean = reference(input);
      validateKey(scope, clean);
      await execute(
        true,
        `DELETE FROM ${table}
        WHERE namespace = $1 AND tenant_id = $2 AND owner_id = $3
          AND document_type = $4 AND document_id = $5`,
        [...scopeValues(scope), clean.type, clean.id],
      );
    },
    async query(value, input: ProviderQuery) {
      const scope = validateScope(value);
      if (!config.enabled) throw new SearchError("disabled");
      if (!object(input)) throw new SearchError("invalid-input");
      keys(input, ["text", "type", "pageSize", "offset", "sort", "includeTotal", "summaryChars"]);
      const queryText = text(input.text, config.maxQueryChars, true).trim();
      const type = input.type === undefined ? null : text(input.type, 64);
      const pageSize = integer(input.pageSize, 1, config.maxPageSize);
      const offset = integer(input.offset, 0, config.maxOffset);
      const summaryChars = integer(input.summaryChars, 1, config.summaryChars);
      if (typeof input.includeTotal !== "boolean") throw new SearchError("invalid-input");
      if (input.sort !== "relevance" && input.sort !== "id")
        throw new SearchError("unsupported-capability");
      if (!queryText)
        return { hits: [], hasMore: false, ...(input.includeTotal ? { total: 0 } : {}) };
      const order =
        input.sort === "relevance"
          ? 'score DESC, document_type COLLATE "C" ASC, document_id COLLATE "C" ASC'
          : 'document_type COLLATE "C" ASC, document_id COLLATE "C" ASC';
      const rows = await execute(
        false,
        `WITH query AS (
          SELECT websearch_to_tsquery($4::regconfig, $5) AS q
        ), authorized AS MATERIALIZED (
          SELECT d.*, query.q FROM ${table} d CROSS JOIN query
          WHERE namespace = $1 AND tenant_id = $2 AND owner_id = $3
            AND language = $4::regconfig
            AND ($6::text IS NULL OR document_type = $6)
            AND search_vector @@ query.q
        ), page AS (
          SELECT *, ts_rank(search_vector, q) AS score FROM authorized
          ORDER BY ${order} LIMIT $7 OFFSET $8
        )
        SELECT COALESCE((SELECT jsonb_agg(hit ORDER BY ${order}) FROM (
          SELECT document_type, document_id, score, jsonb_build_object(
            'id', document_id, 'type', document_type, 'title', title,
            'metadata', metadata, 'score', score,
            'summary', left(ts_headline(language, title || E'\\n' || body, q,
              'StartSel=, StopSel=, MaxWords=35, MinWords=10, MaxFragments=1, FragmentDelimiter= … '), $9)
          ) AS hit FROM page
        ) results), '[]'::jsonb) AS hits
        ${input.includeTotal ? ", (SELECT count(*)::text FROM authorized) AS total" : ""}`,
        [...scopeValues(scope), language, queryText, type, pageSize + 1, offset, summaryChars],
      );
      const hits = rows[0]?.hits as import("./contracts").SearchHit[];
      if (!Array.isArray(hits)) throw new SearchError("query-failed");
      const total = input.includeTotal ? Number(rows[0]?.total) : undefined;
      if (total !== undefined && (!Number.isSafeInteger(total) || total < 0))
        throw new SearchError("query-failed");
      return {
        hits: hits.slice(0, pageSize),
        hasMore: hits.length > pageSize,
        ...(total !== undefined ? { total } : {}),
      };
    },
  };
}
