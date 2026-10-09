/** Trusted host authorization result for one exact partition, never request JSON. */
export interface SearchScope {
  readonly namespace: string;
  readonly tenantId?: string;
  readonly ownerId?: string;
}

export type SearchMetadata = Readonly<Record<string, string | number | boolean | null>>;

export interface SearchReference {
  readonly id: string;
  readonly type: string;
}

export interface SearchDocument extends SearchReference {
  readonly tenantId?: string;
  readonly ownerId?: string;
  readonly title: string;
  readonly body: string;
  readonly metadata?: SearchMetadata;
}

export interface SearchQuery {
  readonly text: string;
  readonly type?: string;
  readonly pageSize?: number;
  readonly cursor?: string;
  readonly sort?: "relevance" | "id";
  readonly includeTotal?: boolean;
}

export interface SearchHit extends SearchReference {
  readonly title: string;
  /** Plain text, not HTML or a safe HTML fragment. Render through a text node. */
  readonly summary: string;
  readonly metadata: SearchMetadata;
  readonly score: number;
}

export interface SearchPage {
  readonly hits: readonly SearchHit[];
  readonly nextCursor?: string;
  readonly total?: number;
}

export interface SearchCapabilities {
  readonly fullText: boolean;
  readonly sorts: readonly ("relevance" | "id")[];
  readonly pagination: "offset";
  readonly summary: "plain-text";
  readonly count: "exact" | "unsupported";
}

export interface SearchConfig {
  readonly enabled?: boolean;
  readonly maxPageSize?: number;
  readonly maxOffset?: number;
  readonly maxQueryChars?: number;
  readonly maxTitleChars?: number;
  readonly maxBodyChars?: number;
  readonly maxDocumentBytes?: number;
  readonly maxMetadataBytes?: number;
  readonly summaryChars?: number;
}

export interface ProviderQuery {
  readonly text: string;
  readonly type?: string;
  readonly pageSize: number;
  readonly offset: number;
  readonly sort: "relevance" | "id";
  readonly includeTotal: boolean;
  readonly summaryChars: number;
}

export interface ProviderPage {
  readonly hits: readonly SearchHit[];
  readonly hasMore: boolean;
  readonly total?: number;
}

/** Provider implementations must independently preserve the exact scope in SQL. */
export interface SearchProvider {
  readonly identity: string;
  readonly capabilities: SearchCapabilities;
  upsert(scope: SearchScope, document: SearchDocument): Promise<void>;
  delete(scope: SearchScope, reference: SearchReference): Promise<void>;
  query(scope: SearchScope, input: ProviderQuery): Promise<ProviderPage>;
}

export interface SearchService {
  readonly capabilities: SearchCapabilities;
  upsert(scope: SearchScope, document: SearchDocument): Promise<void>;
  delete(scope: SearchScope, reference: SearchReference): Promise<void>;
  query(scope: SearchScope, input: SearchQuery): Promise<SearchPage>;
}
