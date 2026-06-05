import { describe, expect, test } from "vitest";

import {
  deadLettersPath,
  functionsPath,
  operationsPath,
  queuesPath,
} from "./operations-url-model";

describe("operations url model", () => {
  test("omits empty query params", () => {
    expect(
      operationsPath("/operations/functions", {
        module: "",
        q: undefined,
        selected: "fn_1",
      })
    ).toBe("/operations/functions?selected=fn_1");
  });

  test("builds function operations paths", () => {
    expect(
      functionsPath({
        moduleName: "identity",
        query: "welcome",
        queue: "runtime",
        selectedId: "fn_1",
        status: "dead",
      })
    ).toBe(
      "/operations/functions?module=identity&q=welcome&queue=runtime&selected=fn_1&status=dead"
    );

    expect(functionsPath({ status: "all" })).toBe("/operations/functions");
  });

  test("builds dead letter operations paths", () => {
    expect(
      deadLettersPath({
        kind: "function",
        oldestFirst: false,
        query: "welcome",
        selectedId: "fn_1",
      })
    ).toBe(
      "/operations/dead-letters?kind=function&order=newest&q=welcome&selected=fn_1"
    );

    expect(deadLettersPath({ kind: "all" })).toBe("/operations/dead-letters");
  });

  test("builds queue operations paths", () => {
    expect(queuesPath()).toBe("/operations/queues");
    expect(
      queuesPath({
        query: "remote",
        selectedId: "runtime.functions:remote-crm",
      })
    ).toBe(
      "/operations/queues?q=remote&selected=runtime.functions%3Aremote-crm"
    );
  });
});
