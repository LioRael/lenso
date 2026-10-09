import { StorageError } from "@lenso/storage";
import { ProcessorError } from "./processor";
import type { MediaErrorCode } from "./contracts";

const messages: Record<MediaErrorCode, string> = {
  "invalid-input": "Media input is invalid.",
  forbidden: "Media access denied.",
  "source-changed": "Source version is no longer available.",
  "not-found": "Media record was not found.",
  "invalid-image": "Image cannot be decoded safely.",
  "unsupported-image": "Image format is not supported.",
  "limit-exceeded": "Image exceeds processing limits.",
  unavailable: "Image processing is unavailable.",
  timeout: "Media processing timed out.",
  cancelled: "Media processing was cancelled.",
  "lease-lost": "Media execution was superseded.",
  dependency: "Media dependency failed; effects may have completed.",
};

export class MediaError extends Error {
  readonly code: MediaErrorCode;
  constructor(code: MediaErrorCode, options?: ErrorOptions) {
    const safe = Object.hasOwn(messages, code) ? code : "dependency";
    super(messages[safe], options);
    this.name = "MediaError";
    this.code = safe;
  }
}
export function mediaErrorDiagnostic(error: unknown) {
  if (!(error instanceof MediaError)) return undefined;
  return { code: error.code, phase: "invoke", message: messages[error.code] } as const;
}
export function classify(error: unknown): MediaError {
  if (error instanceof MediaError) return error;
  if (error instanceof ProcessorError) return new MediaError(error.code, { cause: error });
  if (error instanceof StorageError) {
    const code =
      error.code === "forbidden"
        ? "forbidden"
        : error.code === "too-large"
          ? "limit-exceeded"
          : error.code === "not-found" || error.code === "conflict"
            ? "source-changed"
            : error.code === "aborted"
              ? "cancelled"
              : "dependency";
    return new MediaError(code, { cause: error });
  }
  return new MediaError("dependency", { cause: error });
}
