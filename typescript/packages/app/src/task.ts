export interface TaskCodec<T> {
  encode(value: T): string;
  decode(text: string): T;
}

/** Uses ordinary JSON and one boundary parser; custom formats can still supply their own codec. */
export function jsonCodec<T>(parser: Parser<T>): TaskCodec<T> {
  return {
    encode(value) {
      const parsed = parser.parse(value);
      const text = JSON.stringify(parsed);
      if (text === undefined) {
        if (parsed !== undefined) throw new ActionError("invalid_input", "Value has no JSON representation");
        parser.parse(null);
        return "null";
      }
      return text;
    },
    decode: (text) => parser.parse(JSON.parse(text)),
  };
}

/** Durable payloads explicitly opt into serialization; same-language actions do not. */
export interface Task<I, O, R> {
  readonly id: string;
  readonly input: TaskCodec<I>;
  readonly output: TaskCodec<O>;
  run(context: TaskContext<R>, input: I): O | Promise<O>;
}

/** Fixes resource ownership once while inferring each task's payload and result from its codecs. */
export function defineTask<R>() {
  return <I, O>(definition: Task<I, O, R>): Task<I, O, R> => definition;
}

export interface TaskContext<R> {
  readonly jobId: string;
  readonly attempt: number;
  readonly resources: R;
  readonly signal: AbortSignal;
}
import { ActionError } from "./error.js";
import type { Parser } from "./schema.js";
