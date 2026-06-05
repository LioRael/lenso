import { describe, expect, test } from "vitest";

import { resizeOperationsInspectorWidth } from "./operations-layout";

describe("operations layout", () => {
  test("resizes inspector width opposite to divider drag", () => {
    expect(
      resizeOperationsInspectorWidth({
        currentWidth: 408,
        defaultWidth: 408,
        deltaX: 24,
        maxWidth: 620,
        minWidth: 340,
      })
    ).toBe(384);
  });

  test("falls back to default width and clamps to bounds", () => {
    expect(
      resizeOperationsInspectorWidth({
        currentWidth: undefined,
        defaultWidth: 408,
        deltaX: 200,
        maxWidth: 620,
        minWidth: 340,
      })
    ).toBe(340);

    expect(
      resizeOperationsInspectorWidth({
        currentWidth: 408,
        defaultWidth: 408,
        deltaX: -400,
        maxWidth: 620,
        minWidth: 340,
      })
    ).toBe(620);
  });
});
