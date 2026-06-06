import { describe, expect, test } from "vitest";

import { exampleConsoleManifest, exampleConsoleModule } from ".";

describe("example console package", () => {
  test("declares an installable console package export", () => {
    expect(exampleConsoleManifest).toMatchObject({
      exportName: "exampleConsoleModule",
      packageName: "@lenso/example-console",
      route: "/runtime/example-console",
      surfaceName: "example",
    });
    expect(exampleConsoleModule).toMatchObject({
      id: "example-console",
      surfaces: [
        {
          label: "Example",
          path: "/runtime/example-console",
        },
      ],
    });
  });
});
