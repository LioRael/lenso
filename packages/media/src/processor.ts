export type ImageFormat = "jpeg" | "png" | "webp";

export interface ImageMetadata {
  format: ImageFormat;
  mime: string;
  width: number;
  height: number;
  orientation: number;
  hasAlpha: boolean;
  frames: number;
}

export interface Recipe {
  width: number;
  height: number;
  fit: "cover" | "inside" | "contain";
  format: ImageFormat;
  quality: number;
  metadata: "strip";
  animation: "reject";
}

export interface ProcessorLimits {
  maxInputBytes: number;
  maxWidth: number;
  maxHeight: number;
  maxPixels: number;
  maxFrames: number;
  maxOutputBytes: number;
  timeoutMs: number;
  maxMemoryBytes: number;
  maxTempBytes: number;
  concurrency: number;
}

export interface ImageProcessor {
  readonly version: string;
  inspect(input: Uint8Array, signal: AbortSignal): Promise<ImageMetadata>;
  transform(
    input: Uint8Array,
    recipe: Recipe,
    signal: AbortSignal,
  ): Promise<{ bytes: Uint8Array; metadata: ImageMetadata }>;
}

export type ProcessorErrorCode =
  | "invalid-image"
  | "unsupported-image"
  | "limit-exceeded"
  | "timeout"
  | "cancelled"
  | "unavailable";

export class ProcessorError extends Error {
  constructor(readonly code: ProcessorErrorCode) {
    super(`Image processing failed: ${code}`);
    this.name = "ProcessorError";
  }
}

export const defaultProcessorLimits: Readonly<ProcessorLimits> = Object.freeze({
  maxInputBytes: 20 * 1024 * 1024,
  maxWidth: 8192,
  maxHeight: 8192,
  maxPixels: 32 * 1024 * 1024,
  maxFrames: 1,
  maxOutputBytes: 20 * 1024 * 1024,
  timeoutMs: 10_000,
  maxMemoryBytes: 512 * 1024 * 1024,
  maxTempBytes: 1024 * 1024,
  concurrency: 2,
});
