import { writeBrowserUrl } from "../hooks/use-browser-url-state";

export type OperationsUrlParams = Record<
  string,
  boolean | number | string | null | undefined
>;

export function operationsPath(pathname: string, params: OperationsUrlParams) {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === null || value === undefined || value === "") {
      continue;
    }
    search.set(key, String(value));
  }
  const query = search.toString();
  return query ? `${pathname}?${query}` : pathname;
}

export function readOperationsSearch() {
  if (typeof window === "undefined") {
    return new URLSearchParams();
  }
  return new URLSearchParams(window.location.search);
}

export function readOperationsParam(name: string) {
  return readOperationsSearch().get(name) ?? "";
}

export function replaceOperationsUrl(path: string) {
  writeBrowserUrl(path, "replace");
}

export function pushOperationsUrl(path: string) {
  writeBrowserUrl(path, "push");
}

export function functionsPath(
  filters: {
    moduleName?: string;
    query?: string;
    queue?: string;
    selectedId?: string;
    status?: string;
  } = {}
) {
  return operationsPath("/operations/functions", {
    module: filters.moduleName,
    q: filters.query,
    queue: filters.queue,
    selected: filters.selectedId,
    status: filters.status === "all" ? undefined : filters.status,
  });
}

export function deadLettersPath(
  filters: {
    kind?: string;
    oldestFirst?: boolean;
    query?: string;
    selectedId?: string;
  } = {}
) {
  return operationsPath("/operations/dead-letters", {
    kind: filters.kind === "all" ? undefined : filters.kind,
    order:
      filters.oldestFirst === undefined
        ? undefined
        : filters.oldestFirst
          ? "oldest"
          : "newest",
    q: filters.query,
    selected: filters.selectedId,
  });
}

export function queuesPath(filters: { selectedId?: string } = {}) {
  return operationsPath("/operations/queues", {
    selected: filters.selectedId,
  });
}
