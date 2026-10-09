import { describe, expect, test } from "bun:test";
import { SQL } from "bun";
import { createAuth, defineSource, realm } from "@lenso/auth";
import type { NotesAuthentication } from "../../../examples/notes/src/auth";
import {
  createNotesService,
  notesAudiences,
  type NotesActor,
  type NotesOperation,
  type StoredNote,
} from "../../../examples/notes/src/notes";
import { createSearchService } from "../src/index";
import { createPostgresSearchProvider, postgresSearchMigration } from "../src/postgres";
import { bunSqlSearchDatabase } from "../src/bun-sql";
import { createNotesSearchAdapter, NotesProjectionPending } from "../examples/notes";

const url = process.env.SEARCH_TEST_DATABASE_URL;
(url ? describe : describe.skip)("Notes service with real PostgreSQL projection", () => {
  test("CRUD, current-user query and explicit repair include valid long Notes identities", async () => {
    const suffix = crypto.randomUUID().replaceAll("-", "").slice(0, 20);
    const notesTable = `"notes_owned_${suffix}"`;
    const searchTable = `projection_${suffix}`;
    const client = new SQL({ url: url!, adapter: "postgres" });
    let active = true;
    const owners = ["ordinary", "x".repeat(129), "y".repeat(512), "界".repeat(512)];
    const auth = createAuth(
      realm(
        "notes",
        defineSource<string | null, string>({
          async verify(evidence) {
            return active && evidence !== null && owners.includes(evidence)
              ? { status: "verified", subjectId: evidence, kind: "user" }
              : { status: "rejected" };
          },
        }),
      ),
    );
    const unused = async (): Promise<never> => {
      throw new Error("Unused session method");
    };
    const authentication: NotesAuthentication = {
      ...auth,
      issue: unused,
      renew: unused,
      revoke: unused,
    };
    async function actor<O extends NotesOperation>(
      operation: O,
      owner: string,
    ): Promise<NotesActor<O>> {
      return (await authentication.for(notesAudiences[operation]).required(owner)) as NotesActor<O>;
    }
    function stored(row: Record<string, unknown>): StoredNote {
      return {
        id: String(row.id),
        ownerId: String(row.owner_id),
        title: String(row.title),
        body: String(row.body),
        createdAt: row.created_at as Date,
      };
    }
    try {
      await client.unsafe(`CREATE TABLE ${notesTable} (
        id text PRIMARY KEY, owner_id text NOT NULL, title text NOT NULL,
        body text NOT NULL, created_at timestamptz NOT NULL
      )`);
      await client.unsafe(postgresSearchMigration(searchTable)).simple();
      const notes = createNotesService(
        {
          async insert(note) {
            const [row] = await client.unsafe(
              `INSERT INTO ${notesTable}
            (id, owner_id, title, body, created_at) VALUES ($1,$2,$3,$4,$5) RETURNING *`,
              [note.id, note.ownerId, note.title, note.body, note.createdAt],
            );
            return stored(row);
          },
          async list() {
            throw new Error("Search must not scan Notes");
          },
          async read(id) {
            const [row] = await client.unsafe(`SELECT * FROM ${notesTable} WHERE id=$1`, [id]);
            return row ? stored(row) : null;
          },
          async update(id, owner, input) {
            const [row] = await client.unsafe(
              `UPDATE ${notesTable} SET title=$3,body=$4
            WHERE id=$1 AND owner_id=$2 RETURNING *`,
              [id, owner, input.title, input.body],
            );
            return row ? stored(row) : null;
          },
          async remove(id, owner) {
            const rows = await client.unsafe(
              `DELETE FROM ${notesTable}
            WHERE id=$1 AND owner_id=$2 RETURNING id`,
              [id, owner],
            );
            return rows.length > 0;
          },
        },
        authentication,
      );
      const search = createSearchService({
        provider: createPostgresSearchProvider({
          database: bunSqlSearchDatabase(client),
          table: searchTable,
        }),
        cursorSecret: crypto.getRandomValues(new Uint8Array(32)),
      });
      let failRead = false;
      const adapter = createNotesSearchAdapter({
        namespace: "notes-real-pg",
        notes,
        authentication,
        audiences: notesAudiences,
        search,
        async readLatest({ scope, id }) {
          if (failRead) throw new Error("PRIVATE_SQL_CONNECTION_SENTINEL");
          const [row] = await client.unsafe(
            `SELECT * FROM ${notesTable}
            WHERE id=$1 AND owner_id=$2`,
            [id, scope.ownerId],
          );
          return row ? { ...stored(row), createdAt: stored(row).createdAt.toISOString() } : null;
        },
      });
      const secret = "PRIVATE_OTHER_OWNER_667799";
      const hidden = await adapter.notes.create(await actor("create", owners[0]!), {
        title: "meeting",
        body: secret,
      });
      for (const owner of owners.slice(1)) {
        const created = await adapter.notes.create(await actor("create", owner), {
          title: "meeting",
          body: "authorizedcontent",
        });
        const page = await adapter.query(await actor("list", owner), {
          text: "meeting",
          includeTotal: true,
        });
        expect(page.total).toBe(1);
        expect(page.hits[0]?.id).toBe(created.id);
        expect(JSON.stringify(page)).not.toContain(secret);
        await adapter.notes.update(await actor("update", owner), created.id, {
          title: "updatedmeeting",
          body: "latestcontent",
        });
        expect(
          (await adapter.query(await actor("list", owner), { text: "latestcontent" })).hits[0]?.id,
        ).toBe(created.id);
        // Direct repair rechecks Auth and selects only this current owner's partition.
        await adapter.repair(await actor("read", owner), created.id);
        await adapter.repair(await actor("read", owner), hidden.id);
        expect(
          (await adapter.query(await actor("list", owners[0]!), { text: secret })).hits,
        ).toHaveLength(1);
        expect(await adapter.notes.remove(await actor("remove", owner), created.id)).toBe(true);
        expect(
          (await adapter.query(await actor("list", owner), { text: "latestcontent" })).hits,
        ).toEqual([]);
      }
      failRead = true;
      const pending = await adapter.notes
        .create(await actor("create", owners[0]!), {
          title: "repairmarker",
          body: "committed-only",
        })
        .catch((error: unknown) => error);
      expect(pending).toBeInstanceOf(NotesProjectionPending);
      if (!(pending instanceof NotesProjectionPending))
        throw new Error("Expected pending projection");
      expect(pending.documentId).toBeDefined();
      expect(JSON.stringify(pending)).not.toContain("PRIVATE_SQL_CONNECTION_SENTINEL");
      expect(JSON.stringify(pending)).not.toContain("committed-only");
      expect(pending.repairTicket).toBeUndefined();
      failRead = false;
      const reader = await actor("read", owners[0]!);
      active = false;
      await expect(adapter.repair(reader, pending.documentId!)).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
      active = true;
      await adapter.repair(await actor("read", owners[0]!), pending.documentId!);
      expect(
        (await adapter.query(await actor("list", owners[0]!), { text: "repairmarker" })).hits,
      ).toHaveLength(1);
    } finally {
      await auth.close();
      try {
        await client.unsafe(`DROP TABLE IF EXISTS "${searchTable}"`);
        await client.unsafe(`DROP TABLE IF EXISTS ${notesTable}`);
      } finally {
        await client.close();
      }
    }
  });
});
