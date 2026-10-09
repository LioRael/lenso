import type { MediaArtifact, MediaRecord, MediaStore } from "./contracts";

interface D1Result {
  success: boolean;
  meta?: { changes?: number };
}

interface D1Statement {
  bind(...values: unknown[]): D1Statement;
  first<T = Record<string, unknown>>(columnName?: string): Promise<T | null>;
  run(): Promise<D1Result>;
  all<T = Record<string, unknown>>(): Promise<{ results: T[]; success: boolean }>;
}

export interface D1Database {
  prepare(query: string): D1Statement;
  withSession(constraintOrBookmark?: string): unknown;
}

export function createD1MediaStore(database: D1Database): MediaStore {
  if (typeof database?.prepare !== "function" || typeof database.withSession !== "function") {
    throw new TypeError("A plain D1 binding is required");
  }
  return {
    async insert(record) {
      if (!validRevision(record.revision) || !record.id)
        throw new TypeError("Invalid media identity or revision");
      const result = await database
        .prepare("INSERT OR IGNORE INTO media (id, revision, record) VALUES (?, ?, ?)")
        .bind(record.id, record.revision, JSON.stringify(record))
        .run();
      return changes(result) > 0;
    },
    async get(id) {
      const row = await database
        .prepare("SELECT record FROM media WHERE id = ?")
        .bind(id)
        .first<{ record: string }>();
      return row ? (JSON.parse(row.record) as MediaRecord) : null;
    },
    async replace(id, revision, next) {
      if (next.id !== id || !validNext(revision, next.revision))
        throw new TypeError("Invalid media replacement identity or revision");
      const result = await database
        .prepare("UPDATE media SET revision = ?, record = ? WHERE id = ? AND revision = ?")
        .bind(next.revision, JSON.stringify(next), id, revision)
        .run();
      return changes(result) > 0;
    },
    async insertArtifact(artifact) {
      if (!artifact.fileId || !artifact.derivationId || !validRevision(artifact.revision))
        throw new TypeError("Invalid artifact identity or revision");
      const result = await database
        .prepare(
          "INSERT INTO media_artifacts (file_id, derivation_id, revision, record) VALUES (?, ?, ?, ?)",
        )
        .bind(artifact.fileId, artifact.derivationId, artifact.revision, JSON.stringify(artifact))
        .run();
      if (changes(result) !== 1) throw new Error("D1 artifact was not inserted");
    },
    async getArtifact(fileId) {
      const row = await database
        .prepare("SELECT record FROM media_artifacts WHERE file_id = ?")
        .bind(fileId)
        .first<{ record: string }>();
      return row ? (JSON.parse(row.record) as MediaArtifact) : null;
    },
    async artifacts(derivationId) {
      const result = await database
        .prepare("SELECT record FROM media_artifacts WHERE derivation_id = ? ORDER BY file_id")
        .bind(derivationId)
        .all<{ record: string }>();
      if (!result.success) throw new Error("D1 artifact query failed");
      return result.results.map(({ record }) => JSON.parse(record) as MediaArtifact);
    },
    async replaceArtifact(fileId, revision, next) {
      if (next.fileId !== fileId || !validNext(revision, next.revision))
        throw new TypeError("Invalid artifact replacement identity or revision");
      const result = await database
        .prepare(
          "UPDATE media_artifacts SET derivation_id = ?, revision = ?, record = ? WHERE file_id = ? AND revision = ?",
        )
        .bind(next.derivationId, next.revision, JSON.stringify(next), fileId, revision)
        .run();
      return changes(result) > 0;
    },
  };
}

function changes(result: D1Result): number {
  if (!result.success) throw new Error("D1 statement failed");
  if (typeof result.meta?.changes !== "number")
    throw new Error("D1 did not report affected row count");
  return result.meta.changes;
}

function validRevision(revision: number): boolean {
  return Number.isSafeInteger(revision) && revision >= 0;
}

function validNext(revision: number, next: number): boolean {
  return validRevision(revision) && Number.isSafeInteger(next) && next === revision + 1;
}
