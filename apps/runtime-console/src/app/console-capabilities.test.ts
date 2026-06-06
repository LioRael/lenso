import { describe, expect, test } from "vitest";

import { consoleCapabilityProvider } from "./console-capabilities";

describe("console capabilities", () => {
  test("exposes the current host console capabilities", () => {
    expect(consoleCapabilityProvider()).toEqual(["runtime.stories.read"]);
  });
});
