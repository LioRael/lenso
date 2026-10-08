import { expect, test } from "bun:test";
import { createAuthorization } from "../src/core";
import { predicate } from "../src/conditions";
import { authorizeList } from "../src/list";
import type { Resource } from "../src/types";

const candidates: Resource[] = ["hidden-a", "visible-a", "hidden-b", "visible-b"].map((id) => ({
  id,
  type: "note",
  scope: { type: "personal", id: "home" },
}));
const request = { principal: null, action: "read", context: {} };
const engine = createAuthorization({
  actions: ["read"],
  rules: [
    {
      id: "public",
      effect: "allow",
      actions: ["read"],
      resourceType: "note",
      when: predicate((facts) => facts.resource.id.startsWith("visible")),
    },
  ],
});

test("paginate after authorization; total counts visible complete set only", async () => {
  const result = await authorizeList(engine, candidates, request, {
    maxCandidates: 4,
    offset: 1,
    limit: 1,
  });
  expect(result.items.map((item) => item.id)).toEqual(["visible-b"]);
  expect(result.total).toBe(2);
  expect(JSON.stringify(result)).not.toContain("hidden");
});

test("incomplete/unbounded fallback refuses, not silent partial list", async () => {
  await expect(authorizeList(engine, candidates, request, { maxCandidates: 3 })).rejects.toThrow(
    "Access denied.",
  );
  await expect(
    authorizeList(engine, candidates, request, { maxCandidates: 4, limit: -1 }),
  ).rejects.toThrow("Access denied.");
});

test("failed/unknown/aborted evaluation returns no partial results", async () => {
  const failing = createAuthorization({
    actions: ["read"],
    policies: [
      {
        evaluate(facts) {
          if (facts.resource.id === "hidden-b") throw new Error("private backend");
          return "allow";
        },
      },
    ],
  });
  await expect(authorizeList(failing, candidates, request, { maxCandidates: 4 })).rejects.toThrow(
    "Access denied.",
  );
  await expect(
    authorizeList(engine, candidates, { ...request, action: "unknown" }, { maxCandidates: 4 }),
  ).rejects.toThrow("Access denied.");
});
