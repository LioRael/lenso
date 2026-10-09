import type { Plugin } from "@lenso/core/plugin";
import { bindConfig, definePluginConfig, type ConfigSource } from "@lenso/core/config";
import {
  createSearchService,
  resolveSearchConfig,
  type SearchConfig,
  type SearchService,
} from "./index";
import { createPostgresSearchProvider, searchTable, type SearchDatabase } from "./postgres";
import { keys, object } from "./validation";
import { SearchError } from "./errors";

export interface SearchPluginConfig extends SearchConfig {
  readonly table?: string;
  readonly language?: "simple" | "english";
}

function resolve(value: unknown) {
  if (!object(value)) throw new SearchError("invalid-input");
  keys(value, [
    "table",
    "language",
    "enabled",
    "maxPageSize",
    "maxOffset",
    "maxQueryChars",
    "maxTitleChars",
    "maxBodyChars",
    "maxDocumentBytes",
    "maxMetadataBytes",
    "summaryChars",
  ]);
  const { table = "lenso_search_documents", language = "simple", ...limits } = value;
  if (typeof table !== "string") throw new SearchError("invalid-input");
  searchTable(table);
  if (language !== "simple" && language !== "english") throw new SearchError("invalid-input");
  return { table, language: language as "simple" | "english", ...resolveSearchConfig(limits) };
}

export const searchConfig = definePluginConfig({
  description: "Exact-scope PostgreSQL keyword search. No schema setup or query exposure.",
  schema: {
    "~standard": {
      version: 1 as const,
      vendor: "lenso-search",
      types: undefined as
        | { input: SearchPluginConfig; output: ReturnType<typeof resolve> }
        | undefined,
      validate(value: unknown) {
        try {
          return { value: resolve(value) };
        } catch {
          return { issues: [{ message: "Invalid search configuration" }] };
        }
      },
    },
  },
  fields: [
    { path: ["enabled"], description: "Disable reads and writes without closing the borrowed DB." },
    { path: ["table"], description: "Validated host-owned table name." },
    { path: ["language"], description: "PostgreSQL simple or english configuration." },
  ],
});

export function createPostgresSearchPlugin<TDatabase>(options: {
  readonly id: string;
  readonly database: Plugin<TDatabase>;
  readonly adapter: (database: TDatabase) => SearchDatabase;
  readonly config: SearchPluginConfig | readonly ConfigSource[];
  readonly cursorSecret: Uint8Array;
}): Plugin<SearchService> {
  return bindConfig(searchConfig, options.config, {
    id: options.id,
    requires: [options.database],
    setup(context, config) {
      const { table, language, ...limits } = config;
      const provider = createPostgresSearchProvider({
        database: options.adapter(context.get(options.database)),
        table,
        language,
        config: limits,
      });
      return createSearchService({ provider, cursorSecret: options.cursorSecret, config: limits });
    },
  });
}
