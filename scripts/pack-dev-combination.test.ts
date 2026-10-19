import { expect, test } from "bun:test";
import { combinationManifest } from "./pack-dev-combination";

test("local combination normalization pins all selected dependency kinds without changing identity or third-party metadata", () => {
  const source = {
    name: "@lenso/cli",
    version: "0.21.0",
    dependencies: { "@lenso/engine": "workspace:^", zod: "4.6.5" },
    peerDependencies: { "@lenso/core": "^0.3.1" },
    devDependencies: { "@lenso/engine": "^0.7.0" },
  };
  const result = combinationManifest(
    source,
    new Map([
      ["@lenso/engine", "0.7.0"],
      ["@lenso/core", "0.3.1"],
    ]),
  );
  expect(result).toEqual({
    ...source,
    dependencies: { "@lenso/engine": "0.7.0", zod: "4.6.5" },
    peerDependencies: { "@lenso/core": "0.3.1" },
    devDependencies: { "@lenso/engine": "0.7.0" },
  });
  expect(source.dependencies["@lenso/engine"]).toBe("workspace:^");
  expect(() =>
    combinationManifest(
      { name: "example", version: "1.0.0", dependencies: { unknown: "workspace:*" } },
      new Map(),
    ),
  ).toThrow("Unresolved local dependency");
});
