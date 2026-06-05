import { describe, expect, test, vi } from "vitest";

import {
  applyOperationsUrlBindings,
  readOperationsParamValue,
} from "./operations-url-state";

describe("operations url state", () => {
  test("reads string params from the current URL", () => {
    vi.stubGlobal("window", {
      location: { search: "?q=remote&selected=fn_1" },
    });

    expect(readOperationsParamValue("q")).toBe("remote");
    expect(readOperationsParamValue("selected")).toBe("fn_1");

    vi.unstubAllGlobals();
  });

  test("parses missing params through the provided parser", () => {
    vi.stubGlobal("window", {
      location: { search: "" },
    });

    expect(
      readOperationsParamValue("status", (value) =>
        value === "failed" ? "failed" : "all"
      )
    ).toBe("all");

    vi.unstubAllGlobals();
  });

  test("applies popstate bindings with parsers", () => {
    const values: Record<string, unknown> = {};

    applyOperationsUrlBindings(
      [
        { name: "q", setValue: (value) => (values.q = value) },
        {
          name: "status",
          parse: (value) => (value === "failed" ? "failed" : "all"),
          setValue: (value) => (values.status = value),
        },
        {
          name: "selected",
          setValue: (value) => (values.selected = value),
        },
      ],
      new URLSearchParams("?q=remote&status=failed")
    );

    expect(values).toEqual({
      q: "remote",
      selected: "",
      status: "failed",
    });
  });
});
