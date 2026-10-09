import type { Database } from "bun:sqlite";
import type { MediaArtifact, MediaRecord, MediaStore } from "./contracts";

export function createSqliteMediaStore(database: Database): MediaStore {
  return {
    async insert(record) {
      if (!validRevision(record.revision) || record.id === "")
        throw new TypeError("Invalid media identity or revision");
      return (
        database
          .query("INSERT OR IGNORE INTO media (id, revision, record) VALUES (?, ?, ?)")
          .run(record.id, record.revision, JSON.stringify(record)).changes > 0
      );
    },
    async get(id) {
      const row = database
        .query<{ record: string }, [string]>("SELECT record FROM media WHERE id = ?")
        .get(id);
      return row ? (JSON.parse(row.record) as MediaRecord) : null;
    },
    async replace(id, revision, next) {
      if (next.id !== id || !validNext(revision, next.revision))
        throw new TypeError("Invalid media replacement identity or revision");
      return (
        database
          .query("UPDATE media SET revision = ?, record = ? WHERE id = ? AND revision = ?")
          .run(next.revision, JSON.stringify(next), id, revision).changes > 0
      );
    },
    async insertArtifact(artifact) {
      if (!artifact.fileId || !artifact.derivationId || !validRevision(artifact.revision))
        throw new TypeError("Invalid artifact identity or revision");
      database
        .query(
          "INSERT INTO media_artifacts (file_id, derivation_id, revision, record) VALUES (?, ?, ?, ?)",
        )
        .run(artifact.fileId, artifact.derivationId, artifact.revision, JSON.stringify(artifact));
    },
    async getArtifact(fileId) {
      const row = database
        .query<{ record: string }, [string]>("SELECT record FROM media_artifacts WHERE file_id = ?")
        .get(fileId);
      return row ? (JSON.parse(row.record) as MediaArtifact) : null;
    },
    async artifacts(derivationId) {
      return database
        .query<{ record: string }, [string]>(
          "SELECT record FROM media_artifacts WHERE derivation_id = ? ORDER BY file_id",
        )
        .all(derivationId)
        .map(({ record }) => JSON.parse(record) as MediaArtifact);
    },
    async replaceArtifact(fileId, revision, next) {
      if (next.fileId !== fileId || !validNext(revision, next.revision))
        throw new TypeError("Invalid artifact replacement identity or revision");
      return (
        database
          .query(
            "UPDATE media_artifacts SET derivation_id = ?, revision = ?, record = ? WHERE file_id = ? AND revision = ?",
          )
          .run(next.derivationId, next.revision, JSON.stringify(next), fileId, revision).changes > 0
      );
    },
  };
}

function validRevision(revision: number): boolean {
  return Number.isSafeInteger(revision) && revision >= 0;
}

function validNext(revision: number, next: number): boolean {
  return validRevision(revision) && Number.isSafeInteger(next) && next === revision + 1;
}
