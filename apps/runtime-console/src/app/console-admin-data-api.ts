import { useQuery } from "@tanstack/react-query";

import { httpClient, isApiMode } from "../lib/http-client";

export type ConsoleAdminRecord = Record<string, unknown>;

export type ConsoleAdminListResponse = {
  data: ConsoleAdminRecord[];
  page: {
    limit: number;
    next_cursor: string | null;
  };
};

export function useConsoleAdminRecords({
  entityName,
  limit = 50,
  moduleName,
}: {
  moduleName: string;
  entityName: string;
  limit?: number;
}) {
  return useQuery({
    enabled: isApiMode(),
    queryKey: ["admin-data", "list", moduleName, entityName, limit] as const,
    queryFn: () =>
      httpClient
        .get(
          `admin/data/${encodeURIComponent(moduleName)}/${encodeURIComponent(entityName)}?limit=${limit}`
        )
        .json<ConsoleAdminListResponse>(),
  });
}
