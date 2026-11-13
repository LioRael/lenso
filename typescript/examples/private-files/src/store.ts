import { createHash, randomBytes } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { ActionError } from "@lenso/app";
import type { CallRecord, Principal } from "@lenso/app";

export const userInfo = z.object({ id: z.string(), name: z.string() });
export const fileInfo = z.object({ id: z.string(), owner: z.string(), name: z.string() });
export type User = z.infer<typeof userInfo>;
export type FileInfo = z.infer<typeof fileInfo>;

export function tokenHash(token: string): string {
  return createHash("sha256").update(token).digest("hex");
}

/** The example stores metadata and file bytes in one SQLite transaction domain. */
export class Store {
  readonly db: DatabaseSync;

  constructor(filename: string, private readonly consoleSubject?: string) {
    this.db = new DatabaseSync(filename, { timeout: 5000 });
    this.db.exec(`
      PRAGMA foreign_keys = ON;
      PRAGMA journal_mode = WAL;
      CREATE TABLE IF NOT EXISTS users (
        id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
        salt TEXT NOT NULL, password_hash TEXT NOT NULL
      );
      CREATE TABLE IF NOT EXISTS sessions (
        token_hash TEXT PRIMARY KEY, subject TEXT NOT NULL REFERENCES users(id), expires_ms INTEGER NOT NULL
      );
      CREATE TABLE IF NOT EXISTS files (
        id TEXT PRIMARY KEY, owner TEXT NOT NULL REFERENCES users(id), name TEXT NOT NULL, content BLOB NOT NULL
      );
      CREATE TABLE IF NOT EXISTS calls (
        seq INTEGER PRIMARY KEY, call_id TEXT NOT NULL, action TEXT NOT NULL,
        subject TEXT, outcome TEXT NOT NULL, duration_ms REAL NOT NULL
      );
    `);
  }

  addUser(name: string, salt: string, passwordHash: string): User {
    const id = randomBytes(16).toString("hex");
    const result = this.db.prepare("INSERT INTO users (id, name, salt, password_hash) VALUES (?, ?, ?, ?) ON CONFLICT(name) DO NOTHING")
      .run(id, name, salt, passwordHash);
    if (result.changes === 0) throw new ActionError("conflict", "User already exists");
    return { id, name };
  }

  credentials(name: string) {
    const row = this.db.prepare("SELECT id, salt, password_hash FROM users WHERE name = ?").get(name);
    if (!row) return undefined;
    if (typeof row.id !== "string" || typeof row.salt !== "string" || typeof row.password_hash !== "string") {
      throw new Error("Invalid stored credentials");
    }
    return { id: row.id, salt: row.salt, hash: row.password_hash };
  }

  session(subject: string): string {
    const token = randomBytes(32).toString("hex");
    this.db.prepare("INSERT INTO sessions (token_hash, subject, expires_ms) VALUES (?, ?, ?)")
      .run(tokenHash(token), subject, Date.now() + 86_400_000);
    return token;
  }

  authenticate(token: string | undefined): Principal | undefined {
    if (!token) return undefined;
    const row = this.db.prepare("SELECT subject FROM sessions WHERE token_hash = ? AND expires_ms > ?")
      .get(tokenHash(token), Date.now());
    if (!row) return undefined;
    if (typeof row.subject !== "string") throw new Error("Invalid stored subject");
    return { subject: row.subject, scopes: ["files:read", "files:write", ...(row.subject === this.consoleSubject ? ["console:read"] : [])] };
  }

  upload(owner: string, name: string, content: Uint8Array): FileInfo {
    const id = randomBytes(16).toString("hex");
    this.db.prepare("INSERT INTO files (id, owner, name, content) VALUES (?, ?, ?, ?)").run(id, owner, name, content);
    return { id, owner, name };
  }

  read(id: string) {
    const row = this.db.prepare("SELECT id, owner, name, content FROM files WHERE id = ?").get(id);
    if (!row) throw new ActionError("not_found", "File not found");
    if (typeof row.id !== "string" || typeof row.owner !== "string" || typeof row.name !== "string" || !(row.content instanceof Uint8Array)) {
      throw new Error("Invalid stored file");
    }
    return { id: row.id, owner: row.owner, name: row.name, content: row.content };
  }

  list(owner: string) {
    return this.db.prepare("SELECT id, owner, name FROM files WHERE owner = ? ORDER BY id").all(owner).map((row) => fileInfo.parse(row));
  }

  users() { return this.db.prepare("SELECT id, name FROM users ORDER BY id LIMIT 100").all().map((row) => userInfo.parse(row)); }
  files() { return this.db.prepare("SELECT id, owner, name FROM files ORDER BY id LIMIT 100").all().map((row) => fileInfo.parse(row)); }
  requests() {
    return this.db.prepare("SELECT call_id, action, subject, outcome, duration_ms FROM calls ORDER BY seq DESC LIMIT 100").all();
  }

  record(record: CallRecord): void {
    this.db.prepare("INSERT INTO calls (call_id, action, subject, outcome, duration_ms) VALUES (?, ?, ?, ?, ?)")
      .run(record.callId, record.action, record.subject ?? null, record.outcome, record.durationMs);
  }

  close(): void { this.db.close(); }
}
