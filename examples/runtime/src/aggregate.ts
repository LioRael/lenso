import type { AggregateInput, AggregateOutput } from "../.lenso/runtime/logs";

export class EmptyBatchError extends Error {
  constructor() {
    super("At least one log is required");
  }
}

export function aggregate(input: AggregateInput, signal?: AbortSignal): AggregateOutput {
  if (!input.logs.length) throw new EmptyBatchError();
  let error_count = 0;
  let total_duration_ms = 0;
  for (const record of input.logs) {
    signal?.throwIfAborted();
    if (record.level === "error") error_count++;
    total_duration_ms += record.duration_ms;
  }
  return { count: input.logs.length, error_count, total_duration_ms };
}
