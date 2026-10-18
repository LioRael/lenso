import { Database } from "bun:sqlite";
import { createHmac, randomBytes, timingSafeEqual } from "node:crypto";
import { lstat, mkdir, open as openFile } from "node:fs/promises";
import { join } from "node:path";
import { InitializationError, type InitializationRecord } from "@lenso/core/initialization";

export interface FirstOwnerInput {
  username: string;
  password: string;
  providerKey: string;
}
type Row = { revision: string; request_id: string; activated: number; fingerprint: string };
const ddl = `
  CREATE TABLE IF NOT EXISTS first_run_state (
    id INTEGER PRIMARY KEY CHECK(id=1), revision TEXT NOT NULL,
    request_id TEXT NOT NULL, fingerprint TEXT NOT NULL, activated INTEGER NOT NULL CHECK(activated IN (0,1))
  );
  CREATE TABLE IF NOT EXISTS first_run_owner (
    id INTEGER PRIMARY KEY CHECK(id=1), username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL, role TEXT NOT NULL CHECK(role='owner')
  );
  CREATE TABLE IF NOT EXISTS first_run_config (
    id INTEGER PRIMARY KEY CHECK(id=1), provider_key TEXT NOT NULL, internal_key TEXT NOT NULL
  );`;
const project = (row: Row): InitializationRecord => ({
  revision: row.revision,
  requestId: row.request_id,
  activated: row.activated === 1,
});

/** Application-owned SQLite persistence, not a framework user or credential database. */
export function createNativeFirstRunStore(directory: string) {
  const filename = join(directory, "application.sqlite");
  async function database(create = false): Promise<Database | null> {
    if (create) await mkdir(directory, { recursive: true, mode: 0o700 });
    const folder = await lstat(directory).catch((error: NodeJS.ErrnoException) => {
      if (error.code === "ENOENT") return null;
      throw error;
    });
    if (!folder) return null;
    if (
      !folder.isDirectory() ||
      folder.isSymbolicLink() ||
      (folder.mode & 0o077) !== 0 ||
      folder.uid !== process.getuid?.()
    )
      throw new Error("The local application state directory must be owned and private.");
    if (create) {
      try {
        const file = await openFile(filename, "wx", 0o600);
        await file.close();
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
      }
    }
    const file = await lstat(filename).catch((error: NodeJS.ErrnoException) => {
      if (error.code === "ENOENT") return null;
      throw error;
    });
    if (!file) return null;
    if (
      !file.isFile() ||
      file.isSymbolicLink() ||
      (file.mode & 0o077) !== 0 ||
      file.uid !== process.getuid?.()
    )
      throw new Error("The application state file must be owned and private.");
    const db = new Database(filename, { readonly: !create, strict: true });
    try {
      db.exec("PRAGMA busy_timeout=5000");
      return db;
    } catch (error) {
      db.close();
      throw error;
    }
  }
  function row(db: Database) {
    return db.query<Row, []>("SELECT * FROM first_run_state WHERE id=1").get();
  }
  async function using<T>(create: boolean, call: (db: Database | null) => T): Promise<T> {
    const db = await database(create);
    try {
      return call(db);
    } finally {
      db?.close();
    }
  }
  function fingerprint(input: FirstOwnerInput, key: string) {
    return createHmac("sha256", key)
      .update(JSON.stringify([input.username, input.providerKey]))
      .digest("hex");
  }
  return {
    filename,
    async read(): Promise<InitializationRecord | null> {
      return using(false, (db) => {
        if (!db) return null;
        if (
          !db
            .query("SELECT 1 FROM sqlite_master WHERE type='table' AND name='first_run_state'")
            .get()
        ) {
          if (
            db
              .query("SELECT 1 FROM sqlite_master WHERE type='table' AND name='first_run_owner'")
              .get() &&
            db.query("SELECT 1 FROM first_run_owner LIMIT 1").get()
          )
            throw new InitializationError("initialization-closed");
          return null;
        }
        const current = row(db);
        if (!current && db.query("SELECT 1 FROM first_run_owner LIMIT 1").get())
          throw new InitializationError("initialization-closed");
        return current ? project(current) : null;
      });
    },
    async commit(
      {
        requestId,
        input,
      }: {
        requestId: string;
        input: FirstOwnerInput;
      },
      assertAuthorized: () => void = () => {},
    ): Promise<InitializationRecord> {
      // Slow password hashing precedes the transaction; no resource/owner is acquired yet.
      const passwordHash = await Bun.password.hash(input.password, {
        algorithm: "argon2id",
        memoryCost: 19456,
        timeCost: 2,
      });
      const result = await using(true, (db) => {
        db!.exec("BEGIN IMMEDIATE");
        try {
          assertAuthorized();
          db!.exec(ddl); // Explicit setup apply only; never a request/startup migration.
          const current = row(db!);
          if (current) {
            if (current.request_id !== requestId)
              throw new InitializationError("initialization-closed");
            const config = db!
              .query<{ internal_key: string }, []>(
                "SELECT internal_key FROM first_run_config WHERE id=1",
              )
              .get();
            const expected = fingerprint(input, config!.internal_key);
            if (!timingSafeEqual(Buffer.from(current.fingerprint), Buffer.from(expected)))
              throw new InitializationError("initialization-conflict");
            const owner = db!
              .query<{ password_hash: string }, []>(
                "SELECT password_hash FROM first_run_owner WHERE id=1",
              )
              .get();
            db!.exec("COMMIT");
            return { record: project(current), passwordHash: owner!.password_hash };
          }
          if (db!.query("SELECT 1 FROM first_run_owner LIMIT 1").get())
            throw new InitializationError("initialization-closed");
          const internalKey = randomBytes(32).toString("base64url");
          const revision = crypto.randomUUID();
          db!
            .query("INSERT INTO first_run_owner VALUES (1,?,?, 'owner')")
            .run(input.username, passwordHash);
          db!
            .query("INSERT INTO first_run_config VALUES (1,?,?)")
            .run(input.providerKey, internalKey);
          db!
            .query("INSERT INTO first_run_state VALUES (1,?,?,?,0)")
            .run(revision, requestId, fingerprint(input, internalKey));
          db!.exec("COMMIT");
          return { record: { revision, requestId, activated: false }, passwordHash: null };
        } catch (error) {
          db!.exec("ROLLBACK");
          throw error;
        }
      });
      // Never store a fast digest of a password for retry matching: it would
      // undermine the memory-hard password hash if this local DB were stolen.
      if (
        result.passwordHash !== null &&
        !(await Bun.password.verify(input.password, result.passwordHash))
      )
        throw new InitializationError("initialization-conflict");
      return result.record;
    },
    async complete(
      revision: string,
      assertAuthorized: () => void = () => {},
    ): Promise<InitializationRecord> {
      return using(true, (db) => {
        db!.exec("BEGIN IMMEDIATE");
        try {
          assertAuthorized();
          const current = row(db!);
          if (!current || current.revision !== revision)
            throw new InitializationError("initialization-conflict");
          db!
            .query("UPDATE first_run_state SET activated=1 WHERE id=1 AND revision=?")
            .run(revision);
          const result = project(row(db!)!);
          db!.exec("COMMIT");
          return result;
        } catch (error) {
          db!.exec("ROLLBACK");
          throw error;
        }
      });
    },
    async configuration() {
      return using(false, (db) => {
        if (!db) return {};
        const value = db
          .query<{ provider_key: string; internal_key: string; username: string }, []>(
            "SELECT provider_key, internal_key, username FROM first_run_config, first_run_owner WHERE first_run_config.id=1 AND first_run_owner.id=1",
          )
          .get();
        if (!value) return {};
        return {
          providerKey: value.provider_key,
          signingKey: value.internal_key,
          owner: value.username,
        };
      });
    },
    async authenticate(username: string, password: string) {
      const owner = await using(false, (db) =>
        db
          ?.query<{ username: string; password_hash: string; role: string }, []>(
            "SELECT username,password_hash,role FROM first_run_owner WHERE id=1",
          )
          .get(),
      );
      return (
        !!owner &&
        owner.username === username &&
        (await Bun.password.verify(password, owner.password_hash))
      );
    },
  };
}
