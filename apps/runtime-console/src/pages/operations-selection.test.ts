import { describe, expect, test } from "vitest";

import {
  nextOperationSelectedId,
  selectedOperationIndex,
  selectedOperationItem,
} from "./operations-selection";

const rows = [{ id: "a" }, { id: "b" }];
const getId = (row: (typeof rows)[number]) => row.id;

describe("operations selection", () => {
  test("finds selected items and indices", () => {
    expect(selectedOperationItem({ getId, items: rows, selectedId: "b" })).toBe(
      rows[1]
    );
    expect(
      selectedOperationIndex({ getId, items: rows, selectedId: "b" })
    ).toBe(1);
  });

  test("falls back to the first item when selected id is missing", () => {
    expect(
      nextOperationSelectedId({ getId, items: rows, selectedId: "" })
    ).toBe("a");
    expect(
      nextOperationSelectedId({ getId, items: rows, selectedId: "missing" })
    ).toBe("a");
  });

  test("keeps selected id when it is still visible", () => {
    expect(
      nextOperationSelectedId({ getId, items: rows, selectedId: "b" })
    ).toBe("b");
  });

  test("clears selected id when the list is empty", () => {
    expect(nextOperationSelectedId({ getId, items: [], selectedId: "a" })).toBe(
      ""
    );
    expect(selectedOperationIndex({ getId, items: [], selectedId: "a" })).toBe(
      0
    );
  });
});
