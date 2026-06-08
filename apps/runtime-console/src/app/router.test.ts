import { describe, expect, test } from "vitest";

import { rootRedirectPath } from "./router";

describe("runtime console router", () => {
  test("uses a host route as the root entrypoint", () => {
    expect(rootRedirectPath).toBe("/overview");
  });
});
