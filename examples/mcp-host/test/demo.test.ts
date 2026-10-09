import { expect, test } from "bun:test";
import { runDemo } from "../src/demo";

test("official offline MCP client shares the host's one running application", async () => {
  expect(await runDemo()).toEqual({ tools: 2, revisions: 2, starts: 1, stops: 1 });
});
