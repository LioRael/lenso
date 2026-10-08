import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import { createAuthorization } from "../src/core";
import { predicate } from "../src/conditions";

test("prior allow does not authorize stale state: conditional mutation fences concurrent owner change", async () => {
  const db = new Database(":memory:");
  try {
    db.exec(
      "CREATE TABLE notes (id TEXT PRIMARY KEY, owner TEXT, version INTEGER); INSERT INTO notes VALUES ('one', 'alice', 1)",
    );
    const loaded = db
      .query<{ id: string; owner: string; version: number }, []>("SELECT * FROM notes")
      .get()!;
    const authorization = createAuthorization({
      actions: ["write"],
      rules: [
        {
          id: "owner",
          effect: "allow",
          actions: ["write"],
          resourceType: "note",
          when: predicate(
            (facts) => facts.principal?.subjectId === facts.resource.attributes?.owner,
          ),
        },
      ],
    });
    const request = {
      principal: { realmId: "app", subjectId: "alice", kind: "user" },
      action: "write" as const,
      context: {},
      resource: {
        type: "note",
        id: loaded.id,
        scope: { type: "personal", id: "home" },
        attributes: { owner: loaded.owner, version: loaded.version },
      },
    };
    expect(await authorization.can(request)).toBe(true);
    // A concurrent business mutation happens after the check but before the protected write.
    db.query("UPDATE notes SET owner='bob', version=version+1 WHERE id='one'").run();
    const changed = db
      .query("UPDATE notes SET version=version+1 WHERE id=? AND owner=? AND version=?")
      .run(loaded.id, loaded.owner, loaded.version);
    expect(changed.changes).toBe(0);
    const current = db.query<{ owner: string }, []>("SELECT owner FROM notes").get()!;
    expect(
      await authorization.can({
        ...request,
        resource: { ...request.resource, attributes: { owner: current.owner } },
      }),
    ).toBe(false);
  } finally {
    db.close();
  }
});
