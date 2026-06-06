import { describe, expect, test } from "vitest";

import {
  consoleCapabilityProvider,
  parseDevAuthTokenScopes,
} from "./console-capabilities";

describe("console capabilities", () => {
  test("parses scopes from development service tokens", () => {
    expect(
      parseDevAuthTokenScopes(
        "dev-service:admin:runtime.stories.read,remote_crm.contacts.read"
      )
    ).toEqual(["runtime.stories.read", "remote_crm.contacts.read"]);
    expect(
      parseDevAuthTokenScopes("Bearer dev-service:admin:runtime.stories.read")
    ).toEqual(["runtime.stories.read"]);
  });

  test("uses token scopes in API mode", () => {
    expect(
      consoleCapabilityProvider({
        apiMode: true,
        authToken:
          "dev-service:admin:runtime.stories.read,remote_crm.contacts.read",
      })
    ).toEqual(["runtime.stories.read", "remote_crm.contacts.read"]);
  });

  test("keeps local fallback capabilities outside API mode", () => {
    expect(
      consoleCapabilityProvider({
        apiMode: false,
        authToken: "dev-service:admin",
      })
    ).toEqual(["runtime.stories.read"]);
  });
});
