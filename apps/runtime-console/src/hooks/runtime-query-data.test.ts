import { describe, expect, test } from "vitest";

import { runtimeStories } from "../data/mock-runtime";
import { queryDataWithMockFallback } from "./runtime-query-data";

describe("runtime query data helpers", () => {
  test("does not show mock fallback while API data is still loading", () => {
    expect(
      queryDataWithMockFallback({
        apiMode: true,
        data: undefined,
        fallback: runtimeStories,
        isError: false,
      })
    ).toEqual([]);
  });

  test("uses mock fallback outside API mode", () => {
    expect(
      queryDataWithMockFallback({
        apiMode: false,
        data: undefined,
        fallback: runtimeStories,
        isError: false,
      })
    ).toBe(runtimeStories);
  });

  test("keeps mock fallback when API data is unavailable after an error", () => {
    expect(
      queryDataWithMockFallback({
        apiMode: true,
        data: undefined,
        fallback: runtimeStories,
        isError: true,
      })
    ).toBe(runtimeStories);
  });
});
