import { describe, expect, test } from "vitest";

import { listKeyboardAction } from "./use-list-keyboard";

const items = ["a", "b", "c"];

describe("listKeyboardAction", () => {
  test("moves selection with j and k while clamping to bounds", () => {
    expect(
      listKeyboardAction({
        hasModifier: false,
        isTyping: false,
        items,
        key: "j",
        retryEnabled: false,
        selectedIndex: 1,
      })
    ).toEqual({ index: 2, kind: "select" });

    expect(
      listKeyboardAction({
        hasModifier: false,
        isTyping: false,
        items,
        key: "k",
        retryEnabled: false,
        selectedIndex: 0,
      })
    ).toEqual({ index: 0, kind: "select" });
  });

  test("opens and retries the selected item", () => {
    expect(
      listKeyboardAction({
        hasModifier: false,
        isTyping: false,
        items,
        key: "Enter",
        retryEnabled: false,
        selectedIndex: 1,
      })
    ).toEqual({ item: "b", kind: "open" });

    expect(
      listKeyboardAction({
        hasModifier: false,
        isTyping: false,
        items,
        key: "r",
        retryEnabled: true,
        selectedIndex: 2,
      })
    ).toEqual({ item: "c", kind: "retry" });
  });

  test("ignores typing, modifiers, empty lists, and disabled retry", () => {
    expect(
      listKeyboardAction({
        hasModifier: false,
        isTyping: true,
        items,
        key: "j",
        retryEnabled: false,
        selectedIndex: 0,
      })
    ).toBeNull();

    expect(
      listKeyboardAction({
        hasModifier: true,
        isTyping: false,
        items,
        key: "j",
        retryEnabled: false,
        selectedIndex: 0,
      })
    ).toBeNull();

    expect(
      listKeyboardAction({
        hasModifier: false,
        isTyping: false,
        items: [],
        key: "j",
        retryEnabled: false,
        selectedIndex: 0,
      })
    ).toBeNull();

    expect(
      listKeyboardAction({
        hasModifier: false,
        isTyping: false,
        items,
        key: "r",
        retryEnabled: false,
        selectedIndex: 0,
      })
    ).toBeNull();
  });
});
