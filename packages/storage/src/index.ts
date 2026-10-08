import { definePlugin, type Plugin, type PluginContext } from "@lenso/core/plugin";

export type StorageErrorCode =
  | "invalid-key"
  | "invalid-input"
  | "not-found"
  | "forbidden"
  | "unsupported"
  | "conflict"
  | "too-large"
  | "aborted"
  | "provider";

export class StorageError extends Error {
  constructor(
    readonly code: StorageErrorCode,
    message: string,
    options?: ErrorOptions,
  ) {
    super(message, options);
    this.name = "StorageError";
  }
}

export interface StorageCapabilities {
  readonly provider: "local" | "s3" | "r2-binding";
  readonly signedUpload: boolean;
  readonly signedDownload: boolean;
  readonly rangeRead: boolean;
  readonly conditionalRead: boolean;
  readonly pagination: "key" | "opaque";
  readonly uploadCancellation: "abort" | "stream-only";
  readonly uploadRequiresSize: boolean;
  readonly maxUploadBytes?: number;
}

export interface ObjectMetadata {
  key: string;
  size: number;
  contentType: string;
  etag?: string;
  lastModified?: Date;
  versionId?: string;
  customMetadata?: Record<string, string>;
}

/** Uploads create a new key, never replace an existing object. */
export interface PutObjectInput {
  key: string;
  body: ReadableStream<Uint8Array>;
  contentType?: string;
  size?: number;
  maxBytes?: number;
  customMetadata?: Record<string, string>;
  signal?: AbortSignal;
}

export interface ReadObjectOptions {
  range?: { offset: number; length?: number };
  ifMatch?: string;
  signal?: AbortSignal;
}

export interface ObjectDownload {
  metadata: ObjectMetadata;
  body: ReadableStream<Uint8Array>;
  range?: { offset: number; length: number };
}

export interface ListObjectsInput {
  prefix?: string;
  cursor?: string;
  limit?: number;
  signal?: AbortSignal;
}

export interface ObjectPage {
  objects: ObjectMetadata[];
  cursor?: string;
}

/** URLs and headers are temporary credentials. Never log this result. */
export interface SignedObjectLink {
  url: string;
  method: "GET" | "PUT";
  headers: Record<string, string>;
  expiresAt: Date;
  conditions: {
    contentType?: string;
    ifMatch?: string;
    createOnly?: boolean;
    maxBytes?: number;
    sizeEnforcement: "completion-check" | "none";
  };
}

export interface SignUploadInput {
  key: string;
  expiresIn: number;
  contentType: string;
  maxBytes?: number;
}

export interface SignDownloadInput {
  key: string;
  expiresIn: number;
  ifMatch?: string;
}

export interface ObjectStorage {
  readonly id: string;
  readonly capabilities: StorageCapabilities;
  put(input: PutObjectInput): Promise<ObjectMetadata>;
  get(key: string, options?: ReadObjectOptions): Promise<ObjectDownload>;
  head(key: string, options?: { signal?: AbortSignal }): Promise<ObjectMetadata | null>;
  /** Idempotent. "absent-or-deleted" backends cannot atomically report prior existence. */
  delete(
    key: string,
    options?: { signal?: AbortSignal },
  ): Promise<{
    outcome: "deleted" | "not-found" | "absent-or-deleted";
  }>;
  list(input?: ListObjectsInput): Promise<ObjectPage>;
  signUpload(input: SignUploadInput): Promise<SignedObjectLink>;
  signDownload(input: SignDownloadInput): Promise<SignedObjectLink>;
}

export function createStoragePlugin(options: {
  id: string;
  requires?: readonly Plugin<unknown>[];
  setup(context: PluginContext): Omit<ObjectStorage, "id"> | Promise<Omit<ObjectStorage, "id">>;
}): Plugin<ObjectStorage> {
  return definePlugin({
    id: options.id,
    requires: options.requires,
    async setup(context) {
      return { ...(await options.setup(context)), id: options.id };
    },
  });
}

export function validateKey(key: string): void {
  let unsafeCharacter = false;
  for (const character of key) {
    const code = character.charCodeAt(0);
    if (character === "\\" || code < 32 || code === 127) unsafeCharacter = true;
  }
  if (
    !key ||
    new TextEncoder().encode(key).length > 1024 ||
    unsafeCharacter ||
    key.split("/").some((part) => !part || part === "." || part === "..") ||
    key.startsWith("/") ||
    /^[a-zA-Z]:/.test(key)
  )
    throw new StorageError(
      "invalid-key",
      "Object key must be a relative, nonempty path without traversal",
    );
}

export function validateContentType(contentType: string): void {
  if (
    !contentType ||
    contentType.length > 1024 ||
    [...contentType].some(
      (character) => character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127,
    )
  ) {
    throw new StorageError(
      "invalid-input",
      "Content type must be a nonempty header value without control characters",
    );
  }
}

export function validateList(input: ListObjectsInput): number {
  if (input.prefix)
    validateKey(input.prefix.endsWith("/") ? input.prefix.slice(0, -1) : input.prefix);
  const limit = input.limit ?? 100;
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 1000) {
    throw new StorageError("invalid-input", "List limit must be between 1 and 1000");
  }
  return limit;
}

export function validateRange(options: ReadObjectOptions, supported: boolean): void {
  if (!options.range) return;
  if (!supported) throw new StorageError("unsupported", "Range reads are not supported");
  const { offset, length } = options.range;
  if (
    !Number.isSafeInteger(offset) ||
    offset < 0 ||
    (length !== undefined && (!Number.isSafeInteger(length) || length <= 0))
  ) {
    throw new StorageError("invalid-input", "Range needs a nonnegative offset and positive length");
  }
}

export function validateExpiry(expiresIn: number): void {
  if (!Number.isSafeInteger(expiresIn) || expiresIn < 1 || expiresIn > 604800) {
    throw new StorageError(
      "invalid-input",
      "Link expiry must be 1 to 604800 seconds; credentials may expire sooner",
    );
  }
}

export function unsupportedSigning(): never {
  throw new StorageError(
    "unsupported",
    "This adapter cannot issue signed URLs; configure an authorized application Fetch endpoint",
  );
}
