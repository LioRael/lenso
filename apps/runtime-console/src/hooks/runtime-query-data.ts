export function queryDataWithMockFallback<T>({
  apiMode,
  data,
  fallback,
  isError,
}: {
  apiMode: boolean;
  data?: T[] | undefined;
  fallback: T[];
  isError: boolean;
}): T[] {
  if (data) {
    return data;
  }
  return apiMode && !isError ? [] : fallback;
}
