import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { z } from "zod";
import { ActionError, Catalog, ConsoleExtension, Executor, Module, describeAction, describeActions } from "@lenso/app";
import { ConsoleClient } from "@lenso/console";
import { projectsModule } from "../examples/team-desk/dist/projects.js";

const issues = JSON.parse(await readFile(new URL("../../conformance/fixtures/input-issues.json", import.meta.url)));
const expectedModule = JSON.parse(await readFile(new URL("../../conformance/fixtures/action-metadata.json", import.meta.url)));
const context = { callId: "metadata-test", principal: undefined, resources: {}, signal: new AbortController().signal };
const secret = "private-parser-value";
const parser = z.object({
  profile: z.object({ age: z.number({ error: secret }) }).optional(),
  rows: z.array(z.object({ id: z.string({ error: secret }) })).optional(),
  labels: z.record(z.string(), z.number({ error: secret })).optional(),
});

test("native module, Console and Agent descriptions derive from the same Action annotations", () => {
  const { module, pages } = projectsModule();
  assert.deepEqual(module.describe(), expectedModule);
  const catalog = new Catalog();
  catalog.register(module);
  const extension = new ConsoleExtension(new Executor(() => {}), pages);
  const manifest = extension.manifest(catalog);
  assert.deepEqual(manifest.modules[0], expectedModule);
  assert.deepEqual(catalog.describeActions(), describeActions(manifest.modules));
  assert.deepEqual(describeAction(module.exports.actions.create), {
    id: "projects.create", metadata: expectedModule.actionMetadata["projects.create"],
  });
  const snapshot = catalog.describe();
  snapshot[0].actionMetadata["projects.create"].effects.push("not-a-grant");
  assert.deepEqual(catalog.describe()[0], expectedModule);
});

test("legacy manifest shape, unknown metadata and native values remain compatible", async () => {
  const value = { date: new Date(), bytes: new Uint8Array([1]), integer: 17n, callback: () => "native" };
  let authorized = 0;
  const action = { id: "legacy", authorize() { authorized++; }, run: (_, input) => input };
  const module = new Module("legacy", { actions: { action } });
  assert.deepEqual(module.describe(), { id: "legacy", actions: ["legacy"], tasks: [], events: [], resources: [] });
  assert.deepEqual(describeAction(action), { id: "legacy", metadata: null });
  assert.equal(await new Executor(() => {}).call(action, context, value), value);
  assert.equal(authorized, 1);
  const denied = { ...action, metadata: { inputSchema: false, capabilities: [], effects: [] },
    authorize() { throw new ActionError("forbidden", "Still denied"); } };
  await assert.rejects(new Executor(() => {}).call(denied, context, value), { code: "forbidden" });
  assert.equal(await new Executor(() => {}).call({ ...action, metadata: { inputSchema: false } }, context, value), value);
});

for (const fixture of issues.cases) {
  test(`input projection: ${fixture.name}, including native parser normalization`, async () => {
    const records = [];
    const action = { id: "parse", input: parser, metadata: { inputSchema: issues.schema },
      authorize() { assert.fail("invalid input must precede Access"); }, run() { assert.fail("invalid input must not run"); } };
    for (const native of [true, false]) {
      const executor = new Executor((record) => records.push(record));
      const result = native ? executor.call(action, context, fixture.input) : executor.callExternal(action, context, fixture.input);
      await assert.rejects(result, (error) => {
        assert.equal(error.code, "invalid_input");
        assert.deepEqual(error.toJSON().issues, [{ path: fixture.path, code: "invalid_value" }]);
        assert.ok(error.cause);
        assert.ok(!JSON.stringify(error).includes(secret));
        assert.ok(!JSON.stringify(error).includes("private-map-key"));
        assert.ok(!JSON.stringify(error).includes("stack"));
        return true;
      });
    }
    assert.equal(records.length, 2);
    assert.ok(records.every((record) => record.outcome === "invalid_input"));
    assert.ok(!JSON.stringify(records).includes(secret));
  });
}

test("opaque codec errors keep private causes; output errors cannot expose input issues", async () => {
  const executor = new Executor(() => {});
  const action = { id: "opaque", authorize() {}, run: (_, value) => value };
  await assert.rejects(executor.callExternal(action, context, {}, {
    decode() { throw new Error(secret); }, encode: (value) => value,
  }), (error) => {
    assert.deepEqual(error.toJSON(), { code: "invalid_input", message: "Invalid input", issues: [{ path: [], code: "invalid_value" }] });
    assert.equal(error.cause.message, secret);
    return true;
  });
  const opaque = { get issues() { throw new Error(secret); } };
  await assert.rejects(executor.call({ ...action, input: { parse() { throw opaque; } },
    authorize() { assert.fail("invalid input reached authorization"); },
    run() { assert.fail("invalid input reached execution"); },
  }, context, {}), (error) => {
    assert.equal(error.code, "invalid_input");
    assert.equal(error.cause, opaque);
    assert.deepEqual(error.toJSON(), { code: "invalid_input", message: "Invalid input", issues: [{ path: [], code: "invalid_value" }] });
    return true;
  });
  await assert.rejects(executor.call({ ...action, validateOutput() {
    throw new ActionError("invalid_input", secret, { issues: [{ path: [secret], code: "invalid_value" }] });
  } }, context, "ok"), (error) => {
    assert.deepEqual(error.toJSON(), { code: "internal", message: "Invalid output" });
    return true;
  });
});

test("Console HTTP parsing retains metadata and safe input issues for Agent consumers", async () => {
  const catalog = new Catalog();
  const { module, pages } = projectsModule();
  catalog.register(module);
  const manifest = new ConsoleExtension(new Executor(() => {}), pages).manifest(catalog);
  const publicError = { code: "invalid_input", message: "Invalid input", issues: [{ path: ["name"], code: "required" }] };
  const client = new ConsoleClient("https://example.test/console", () => "private-token", async (url) => {
    return url.pathname.endsWith("/manifest") ? Response.json(manifest) : Response.json({ error: { ...publicError, cause: secret, stack: secret } }, { status: 400 });
  });
  assert.deepEqual(await client.describeActions(), catalog.describeActions());
  await assert.rejects(client.call("projects.create", {}), (error) => { assert.deepEqual(error.toJSON(), publicError); return true; });
  assert.deepEqual(new ActionError("forbidden", "Denied").toJSON(), { code: "forbidden", message: "Denied" });
});

test("metadata cannot invent installed Actions and prototype names do not invent metadata", () => {
  const action = { id: "constructor", authorize() {}, run() {} };
  const other = { ...action, id: "other", metadata: { effects: [] } };
  const module = new Module("prototype", { actions: { action, other } });
  assert.deepEqual(describeActions([module.describe()])[0], { id: "constructor", metadata: null });
  assert.throws(() => new ConsoleExtension(new Executor(() => {}), []).manifest({
    describe: () => [{ ...module.describe(), actionMetadata: { missing: { effects: [] } } }],
  }), { code: "not_found" });
});
