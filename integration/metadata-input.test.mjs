import assert from "node:assert/strict";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import { DatabaseSync } from "node:sqlite";
import { describeActions } from "@lenso/app";
import { fixture, identities } from "./helpers/team-desk.mjs";

const expected = JSON.parse(await readFile(new URL("../conformance/fixtures/action-metadata.json", import.meta.url)));
const secret = "sdk-private-input-token";
for (const runtime of ["typescript", "rust"]) {
  test(`${runtime}: HTTP, Console and an Agent consume one description and one error contract`, async () => {
    let report;
    await fixture(async (f) => {
      const server = await f.start(runtime);
      const alice = server.client("alice");
      const manifest = await alice.manifest();
      assert.deepEqual(manifest.modules.find((module) => module.id === "projects"), expected);
      const agentActions = describeActions(manifest.modules);
      assert.deepEqual(await alice.describeActions(), agentActions);
      assert.equal(agentActions.find((a) => a.id === "projects.detail").metadata, null);
      const create = agentActions.find((a) => a.id === "projects.create");
      assert.deepEqual(create.metadata, expected.actionMetadata["projects.create"]);
      assert.deepEqual(create.metadata.inputSchema.properties.name.description, "Project display name");
      assert.deepEqual(create.metadata.effects, ["write:projects"]);
      assert.deepEqual(agentActions.find((a) => a.id === "projects.list").metadata.effects, []);

      const db = new DatabaseSync(f.filename);
      try {
        assert.equal(db.prepare("SELECT count(*) AS n FROM desk_calls WHERE action LIKE 'projects.%'").get().n, 0,
          "description reads must not invoke any project Action");
        assert.equal(db.prepare("SELECT count(*) AS n FROM desk_projects").get().n, 0);
        const headers = { authorization: `Bearer ${identities[0].token}`, "content-type": "application/json" };
        const invalidJson = await fetch(`${server.url}/console/actions/projects.create`, {
          method: "POST", headers, body: '{"private":"' + secret,
        });
        assert.equal(invalidJson.status, 400);
        const invalidJsonBody = await invalidJson.text();
        assert.ok(!invalidJsonBody.includes(secret));
        assert.deepEqual(JSON.parse(invalidJsonBody).error.issues, [{ path: [], code: "invalid_format" }]);
        const response = await fetch(`${server.url}/console/actions/projects.create`, {
          method: "POST", headers, body: JSON.stringify({ id: 17, name: secret, [secret]: secret }),
        });
        assert.equal(response.status, 400);
        const raw = await response.text();
        assert.ok(!raw.includes(secret) && !raw.includes("stack") && !raw.includes("cause"));
        const failure = JSON.parse(raw).error;
        assert.equal(failure.code, "invalid_input");
        assert.deepEqual(failure.issues, [{ path: ["id"], code: "invalid_value" }]);
        await assert.rejects(alice.call(create.id, { id: 17, name: secret }), (error) => {
          assert.deepEqual(error.toJSON(), failure); return true;
        });
        await assert.rejects(alice.call(create.id, { id: "empty", name: "" }), (error) => {
          assert.equal(error.code, "invalid_input");
          assert.deepEqual(error.issues, [{ path: ["name"], code: "invalid_value" }]);
          return true;
        });
        await assert.rejects(server.client("reader").call(create.id, { id: "denied", name: "No grant from metadata" }), { code: "forbidden" });
        await assert.rejects(server.client("unknown").describeActions(), { code: "unauthenticated" });
        assert.equal(db.prepare("SELECT count(*) AS n FROM desk_projects").get().n, 0);
        const created = await alice.call(create.id, { id: "agent-created", name: "  From the shared description  " });
        assert.equal(created.name, "From the shared description");
        assert.equal((await alice.call("projects.detail", { id: created.id })).id, created.id);
        assert.equal(db.prepare("SELECT count(*) AS n FROM desk_projects").get().n, 1);
        assert.equal(db.prepare("SELECT count(*) AS n FROM desk_calls WHERE action='projects.create' AND outcome='ok'").get().n, 1);
        assert.ok(db.prepare("SELECT count(*) AS n FROM desk_calls WHERE action='projects.create' AND outcome='invalid_input'").get().n >= 3);
        report = {
          schema_version: 1, status: "passed", runtime,
          platform: { system: process.platform, machine: process.arch, node: process.version },
          module: manifest.modules.find((module) => module.id === "projects"), invalidInput: failure,
          checks: ["shared native/HTTP/Console/Agent description", "legacy unknown metadata", "discovery does not invoke project Actions",
            "private input/unknown-key/cause exclusion", "structured JSON and field failures", "metadata grants no authority",
            "normalized real write and independent readback", "one successful create recorded"],
          effects: { projects: db.prepare("SELECT count(*) AS n FROM desk_projects").get().n,
            successfulCreates: db.prepare("SELECT count(*) AS n FROM desk_calls WHERE action='projects.create' AND outcome='ok'").get().n },
        };
      } finally { db.close(); }
    });
    if (process.env.LENSO_METADATA_REPORT_DIR) {
      await mkdir(process.env.LENSO_METADATA_REPORT_DIR, { recursive: true });
      await writeFile(join(process.env.LENSO_METADATA_REPORT_DIR, `${runtime}.json`), JSON.stringify(report, null, 2) + "\n");
    }
  });
}
