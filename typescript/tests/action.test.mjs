import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  ActionError, Executor, checkCancellation, requireOwner, requireScope,
} from "@lenso/app";

const fixtures = JSON.parse(await readFile(new URL("../../conformance/fixtures/action.json", import.meta.url)));
const codec = {
  decode(value) {
    if (typeof value !== "object" || value === null || Array.isArray(value)
      || typeof value.owner !== "string" || !Number.isSafeInteger(value.value)) {
      throw new Error("Invalid input shape");
    }
    return value;
  },
  encode: (value) => value,
};

for (const fixture of fixtures) {
  for (const entrypoint of ["external", "native"]) {
    if (entrypoint === "native" && fixture.name === "malformed external input") continue;
    test(`${entrypoint}: ${fixture.name}`, async () => {
      const records = [];
      const controller = new AbortController();
      const executor = new Executor((record) => records.push(record));
      let runs = 0;
      const action = {
        id: "files.read",
        validateInput(input) {
          if (input.value < 0) throw new ActionError("invalid_input", "Negative value");
        },
        async authorize(context, input) {
          requireScope(context.principal, "files:read");
          requireOwner(context.principal, input.owner);
          if (fixture.mode === "cancel_authorize") controller.abort();
        },
        async run(context, input) {
          runs++;
          if (fixture.mode === "fail") throw new Error("private dependency details");
          if (fixture.mode.startsWith("cancel_")) controller.abort();
          if (fixture.mode === "cancel_handler") checkCancellation(context.signal);
          return fixture.mode === "invalid_output" ? -1 : input.value;
        },
        validateOutput(output) {
          if (fixture.mode === "invalid_output_code") throw new ActionError("forbidden", "Invalid output implementation");
          if (output < 0) throw new ActionError("internal", "Invalid output");
        },
      };
      const context = {
        callId: "call-1",
        principal: fixture.subject ? { subject: fixture.subject, scopes: fixture.scopes } : undefined,
        resources: {},
        signal: controller.signal,
      };
      if (fixture.mode === "cancel_before") controller.abort();
      const result = entrypoint === "native"
        ? executor.call(action, context, fixture.input)
        : executor.callExternal(action, context, fixture.input, codec);
      if (fixture.expected === "ok") {
        assert.equal(await result, fixture.input.value);
      } else {
        await assert.rejects(result, (error) => {
          assert.ok(error instanceof ActionError);
          assert.equal(error.code, fixture.expected);
          if (fixture.mode === "fail") {
            assert.equal(error.message, "Action failed");
            assert.equal(error.cause.message, "private dependency details");
          }
          return true;
        });
      }
      assert.equal(runs, fixture.runs);
      assert.equal(records.length, 1);
      assert.deepEqual(Object.keys(records[0]).sort(), ["action", "callId", "durationMs", "outcome", "subject"]);
      assert.equal(records[0].outcome, fixture.expected);
      assert.equal(records[0].subject, fixture.subject);
      assert.ok(records[0].durationMs >= 0);
    });
  }
}

test("native calls retain non-JSON types and object identity", async () => {
  const input = { amount: 10n, date: new Date(), callback: () => "native" };
  const executor = new Executor(() => {});
  const output = await executor.call({
    id: "native.identity", authorize() {}, run: (_context, value) => value,
  }, { callId: "native-1", principal: undefined, resources: {}, signal: new AbortController().signal }, input);
  assert.equal(output, input);
  assert.equal(output.callback(), "native");
});

test("external encoding failures are recorded as internal failures", async () => {
  const records = [];
  const executor = new Executor((record) => records.push(record));
  await assert.rejects(executor.callExternal({
    id: "bad.encode", authorize() {}, run: () => 1,
  }, { callId: "encode-1", principal: undefined, resources: {}, signal: new AbortController().signal }, {}, {
    decode: () => ({}), encode: () => { throw new Error("encoding failed"); },
  }), { code: "internal" });
  assert.equal(records[0].outcome, "internal");
});
