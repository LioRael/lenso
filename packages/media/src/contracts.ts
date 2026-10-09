import type { FileRecord } from "@lenso/storage/files";
import type { ImageFormat, ImageMetadata, ImageProcessor, Recipe } from "./processor";

export interface SourceVersion {
  readonly revision: number;
  readonly etag: string;
}
export interface FileReference {
  readonly fileId: string;
  readonly version: SourceVersion;
}
export interface MediaScope {
  readonly tenantId: string;
  readonly subjectId: string;
  /** Trusted authorization partition, not caller JSON or a content hash. */
  readonly isolationId: string;
}
export interface Preset {
  readonly name: string;
  readonly version: string;
  readonly width: number;
  readonly height: number;
  readonly fit: Recipe["fit"];
  readonly formats: readonly ImageFormat[];
  readonly defaultFormat: ImageFormat;
  readonly quality: number;
  readonly metadata: "strip";
  readonly animation: "reject";
}
export type MediaStage = "queued" | "download" | "decode" | "transform" | "upload" | "register";
export type MediaState = "pending" | "running" | "ready" | "failed" | "cancelled";
export type MediaErrorCode =
  | "invalid-input"
  | "forbidden"
  | "source-changed"
  | "not-found"
  | "invalid-image"
  | "unsupported-image"
  | "limit-exceeded"
  | "unavailable"
  | "timeout"
  | "cancelled"
  | "lease-lost"
  | "dependency";
export interface MediaFailure {
  readonly code: MediaErrorCode;
  readonly stage: MediaStage;
  readonly retryable: boolean;
}
export interface MediaRecord {
  id: string;
  revision: number;
  scope: MediaScope;
  source: FileReference;
  sourceOwnerId: string;
  sourceStorageId: string;
  preset: { name: string; version: string };
  recipe: Recipe | null;
  processorVersion: string;
  queueIdentity: string;
  jobId: string | null;
  fence: number;
  executionId: string | null;
  state: MediaState;
  stage: MediaStage;
  cancelRequested: boolean;
  metadata: ImageMetadata | null;
  result: FileReference | null;
  error: MediaFailure | null;
  createdAt: number;
  updatedAt: number;
}
export interface MediaArtifact {
  fileId: string;
  derivationId: string;
  fence: number;
  executionId: string;
  revision: number;
  state: "staged" | "discarding" | "deleted";
  createdAt: number;
}

/** Durable business state, not a queue. Every mutation must be atomic in the backend. */
export interface MediaStore {
  insert(record: MediaRecord): Promise<boolean>;
  get(id: string): Promise<MediaRecord | null>;
  replace(id: string, revision: number, next: MediaRecord): Promise<boolean>;
  insertArtifact(artifact: MediaArtifact): Promise<void>;
  getArtifact(fileId: string): Promise<MediaArtifact | null>;
  artifacts(derivationId: string): Promise<MediaArtifact[]>;
  replaceArtifact(fileId: string, revision: number, next: MediaArtifact): Promise<boolean>;
}
export interface MediaStorage<Access> {
  source(access: Access, reference: FileReference, signal?: AbortSignal): Promise<FileRecord>;
  download(
    access: Access,
    reference: FileReference,
    maxBytes: number,
    signal: AbortSignal,
  ): Promise<Uint8Array>;
  upload(
    access: Access,
    record: MediaRecord,
    bytes: Uint8Array,
    metadata: ImageMetadata,
    signal: AbortSignal,
  ): Promise<FileReference>;
  result(access: Access, reference: FileReference): Promise<void>;
  /** Uses Files deletion; force recovery of abandoned uploading records is explicit. */
  discard(access: Access, artifact: MediaArtifact): Promise<void>;
}
export interface MediaTasks {
  identity(): Promise<string>;
  ensure(id: string): Promise<string>;
  job(id: string): Promise<{
    state: "pending" | "running" | "succeeded" | "failed" | "cancelled";
    cancelRequested: boolean;
  } | null>;
  cancel(id: string): Promise<unknown>;
  retry(id: string): Promise<boolean>;
}
export interface MediaOptions<Access> {
  store: MediaStore;
  storage: MediaStorage<Access>;
  tasks: MediaTasks;
  presets: readonly Preset[];
  /** Exact processor fingerprint deployed on the separate executor, also on Workers. */
  processorVersion: string;
  processor?: ImageProcessor;
  scope(access: Access): MediaScope | Promise<MediaScope>;
  authorizeDerive(access: Access, source: Readonly<FileRecord>): boolean | Promise<boolean>;
  /** Resolves live, minimal task authority from persisted requester identity. No credentials in jobs. */
  delegate(record: Readonly<MediaRecord>, purpose: "execute" | "cleanup"): Access | Promise<Access>;
  maxInputBytes?: number;
  timeoutMs?: number;
}
export interface MediaRequest {
  source: FileReference;
  preset: string;
  format?: ImageFormat;
}
export interface MediaStatus {
  id: string;
  state: MediaState;
  stage: MediaStage;
  cancelRequested: boolean;
  metadata: ImageMetadata | null;
  result: FileReference | null;
  error: MediaFailure | null;
  cleanupPending: boolean;
}
