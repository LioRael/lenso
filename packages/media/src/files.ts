import type { FileAction, FileQueries, FileRecord, Files } from "@lenso/storage/files";
import { MediaError } from "./errors";
import { validateReference } from "./index";
import type {
  FileReference,
  MediaArtifact,
  MediaRecord,
  MediaStorage,
  MediaStore,
} from "./contracts";

export function fileReference(file: FileRecord): FileReference {
  if (file.state !== "ready" || !file.etag) throw new MediaError("source-changed");
  return { fileId: file.fileId, version: { revision: file.revision, etag: file.etag } };
}
function matches(file: FileRecord, reference: FileReference) {
  return (
    file.state === "ready" &&
    file.revision === reference.version.revision &&
    file.etag === reference.version.etag
  );
}

/** Wrap the existing Files queries, before creating that exact Files instance. */
export function createMediaFileJournal(options: { store: MediaStore; queries: FileQueries }) {
  const staging = new Map<string, MediaRecord>();
  async function active(record: MediaRecord) {
    const current = await options.store.get(record.id);
    if (
      current?.executionId !== record.executionId ||
      current.state !== "running" ||
      current.cancelRequested
    ) {
      throw new MediaError("lease-lost");
    }
  }
  const queries: FileQueries = {
    async insert(file) {
      const record = staging.get(file.filename);
      if (record) {
        await active(record);
        const artifact: MediaArtifact = {
          fileId: file.fileId,
          derivationId: record.id,
          fence: record.fence,
          executionId: record.executionId!,
          revision: 0,
          state: "staged",
          createdAt: Date.now(),
        };
        // Journal first: every provider write has an internal Files identity for recovery.
        await options.store.insertArtifact(artifact);
      }
      await options.queries.insert(file);
    },
    get: (id) => options.queries.get(id),
    async transition(id, revision, next) {
      if (next.state === "ready") {
        const artifact = await options.store.getArtifact(id);
        if (artifact) {
          const record = await options.store.get(artifact.derivationId);
          if (artifact.state !== "staged" || !record || record.executionId !== artifact.executionId)
            return false;
          await active(record);
        }
      }
      return options.queries.transition(id, revision, next);
    },
  };
  async function publication(fileId: string): Promise<MediaRecord | null | undefined> {
    const artifact = await options.store.getArtifact(fileId);
    if (!artifact) return undefined;
    const record = await options.store.get(artifact.derivationId);
    if (
      artifact.state !== "staged" ||
      record?.state !== "ready" ||
      record.result?.fileId !== fileId
    )
      return null;
    return record;
  }
  return {
    queries,
    store: options.store,
    staging(filename: string) {
      const record = staging.get(filename);
      return record ? structuredClone(record) : null;
    },
    async track<T>(filename: string, record: MediaRecord, run: () => Promise<T>): Promise<T> {
      if (staging.has(filename)) throw new MediaError("dependency");
      staging.set(filename, structuredClone(record));
      try {
        return await run();
      } finally {
        staging.delete(filename);
      }
    },
    /** undefined: ordinary file; null: unpublished; record: publication still needs live authorization. */
    publication,
    authorizer<Access>(policy: {
      authorize(input: {
        access: Access;
        action: FileAction;
        file: Readonly<FileRecord>;
      }): boolean | Promise<boolean>;
      authorizeDelivery(access: Access, derivationId: string): Promise<void>;
    }) {
      return async (input: { access: Access; action: FileAction; file: Readonly<FileRecord> }) => {
        if (["metadata", "read", "signDownload"].includes(input.action)) {
          const published = await publication(input.file.fileId);
          if (published === null) return false;
          if (published) {
            if (!matches(input.file, published.result!)) return false;
            await policy.authorizeDelivery(input.access, published.id);
          }
        }
        return policy.authorize(input);
      };
    },
  };
}
export type MediaFileJournal = ReturnType<typeof createMediaFileJournal>;

export function createFilesMediaStorage<Access>(options: {
  files: Files<Access>;
  journal: MediaFileJournal;
  /** Private destination configured by the host, never a caller-selected bucket key. */
  storageId: string;
  maxOutputBytes?: number;
}): MediaStorage<Access> {
  const maxOutputBytes = options.maxOutputBytes ?? 8 * 1024 * 1024;
  if (!options.storageId || !Number.isSafeInteger(maxOutputBytes) || maxOutputBytes < 1) {
    throw new MediaError("invalid-input");
  }
  async function source(access: Access, reference: FileReference, signal?: AbortSignal) {
    validateReference(reference);
    signal?.throwIfAborted();
    // v1 derives only original Files, not recursively chained Media outputs.
    if (await options.journal.store.getArtifact(reference.fileId))
      throw new MediaError("invalid-input");
    const file = await options.files.metadata(access, reference.fileId);
    if (!matches(file, reference)) throw new MediaError("source-changed");
    const download = await options.files.read(access, reference.fileId, { signal });
    try {
      if (
        download.metadata.etag !== reference.version.etag ||
        download.metadata.size !== file.size
      ) {
        throw new MediaError("source-changed");
      }
    } finally {
      await download.body.cancel();
    }
    return file;
  }
  return {
    source,
    async download(access, reference, maxBytes, signal) {
      const file = await source(access, reference, signal);
      if (file.size === null || file.size > maxBytes) throw new MediaError("limit-exceeded");
      const download = await options.files.read(access, reference.fileId, { signal });
      const reader = download.body.getReader();
      const chunks: Uint8Array[] = [];
      let length = 0;
      const abort = () => {
        void reader.cancel().catch(() => {});
      };
      signal.addEventListener("abort", abort, { once: true });
      try {
        if (download.metadata.etag !== reference.version.etag)
          throw new MediaError("source-changed");
        for (;;) {
          signal.throwIfAborted();
          const { done, value } = await reader.read();
          if (done) break;
          length += value.byteLength;
          if (length > maxBytes) throw new MediaError("limit-exceeded");
          chunks.push(value);
        }
        signal.throwIfAborted();
        if (length !== file.size) throw new MediaError("source-changed");
        const bytes = new Uint8Array(length);
        let offset = 0;
        for (const chunk of chunks) {
          bytes.set(chunk, offset);
          offset += chunk.byteLength;
        }
        await source(access, reference, signal);
        return bytes;
      } finally {
        signal.removeEventListener("abort", abort);
        await reader.cancel();
        reader.releaseLock();
      }
    },
    async upload(access, record, bytes, metadata, signal) {
      signal.throwIfAborted();
      if (bytes.byteLength > maxOutputBytes) throw new MediaError("limit-exceeded");
      const filename = `media-${record.id}-${record.executionId}.${metadata.format}`;
      const file = await options.journal.track(filename, record, () =>
        options.files.upload(access, {
          storageId: options.storageId,
          filename,
          contentType: metadata.mime,
          ownerId: record.sourceOwnerId,
          tenantId: record.scope.tenantId,
          size: bytes.byteLength,
          maxBytes: maxOutputBytes,
          signal,
          body: new Blob([new Uint8Array(bytes)]).stream(),
        }),
      );
      signal.throwIfAborted();
      return fileReference(file);
    },
    async result(access, reference) {
      const file = await options.files.metadata(access, reference.fileId);
      if (!matches(file, reference)) throw new MediaError("source-changed");
    },
    async discard(access, artifact) {
      const saved = await options.journal.store.getArtifact(artifact.fileId);
      const record = await options.journal.store.get(artifact.derivationId);
      if (
        saved?.state !== "discarding" ||
        record?.result?.fileId === artifact.fileId ||
        record?.executionId === artifact.executionId
      )
        throw new MediaError("lease-lost");
      let file = await options.journal.queries.get(artifact.fileId);
      if (!file) return;
      if (file.state === "uploading" || file.state === "deleted") {
        // Revoke abandoned publication before using the existing Files deletion state machine.
        // Deleted tombstones are revisited because an old provider PUT can settle late.
        const next = {
          ...file,
          state: file.state === "uploading" ? ("failed" as const) : ("deleting" as const),
          revision: file.revision + 1,
          updatedAt: Date.now(),
        };
        if (!(await options.journal.queries.transition(file.fileId, file.revision, next))) {
          throw new MediaError("dependency");
        }
        file = next;
      }
      await options.files.delete(access, file.fileId);
    },
  };
}
