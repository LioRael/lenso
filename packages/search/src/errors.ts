export type SearchErrorCode =
  | "invalid-input"
  | "missing-scope"
  | "scope-conflict"
  | "unsupported-capability"
  | "disabled"
  | "invalid-cursor"
  | "db-unavailable"
  | "index-failed"
  | "query-failed";

const messages: Record<SearchErrorCode, string> = {
  "invalid-input": "Invalid search input",
  "missing-scope": "An explicit search scope is required",
  "scope-conflict": "Document and authorized scope conflict",
  "unsupported-capability": "Search capability is not supported",
  disabled: "Search is disabled",
  "invalid-cursor": "Invalid search cursor",
  "db-unavailable": "Search database is unavailable",
  "index-failed": "Search projection write failed",
  "query-failed": "Search query failed",
};

export class SearchError extends Error {
  constructor(readonly code: SearchErrorCode) {
    super(messages[code]);
    this.name = "SearchError";
  }
}

export function searchErrorDiagnostic(error: unknown) {
  return error instanceof SearchError
    ? { code: error.code, message: error.message }
    : { code: "query-failed" as const, message: messages["query-failed"] };
}
