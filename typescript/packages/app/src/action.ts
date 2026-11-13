import type { Principal } from "./access.js";
import { ActionError, actionError } from "./error.js";
import type { ErrorCode } from "./error.js";
import type { Parser } from "./schema.js";

export interface Context<R> {
  readonly callId: string;
  readonly principal: Principal | undefined;
  readonly resources: R;
  readonly signal: AbortSignal;
}

/** Native values stay native; optional parsers validate and normalize them without serialization. */
export interface Action<I, O, R> {
  readonly id: string;
  readonly input?: Parser<I>;
  readonly output?: Parser<O>;
  validateInput?(input: I): void | Promise<void>;
  authorize(context: Context<R>, input: I): void | Promise<void>;
  run(context: Context<R>, input: I): O | Promise<O>;
  validateOutput?(output: O): void | Promise<void>;
}

/** Fixes the native resource type once; input and output types follow each definition. */
export function defineAction<R>() {
  function action<I, O>(definition: Omit<Action<I, O, R>, "run"> & {
    output: Parser<O>;
    run(context: Context<R>, input: I): NoInfer<O> | Promise<NoInfer<O>>;
  }): Action<I, O, R>;
  function action<I, O>(definition: Omit<Action<I, O, R>, "output"> & { output?: never }): Action<I, O, R>;
  function action<I, O>(definition: Action<I, O, R>): Action<I, O, R> { return definition; }
  return action;
}

export interface CallRecord {
  readonly callId: string;
  readonly action: string;
  readonly subject: string | undefined;
  readonly outcome: "ok" | ErrorCode;
  readonly durationMs: number;
}

/** A transport codec opts into serialization without constraining native types. */
export interface Codec<I, O> {
  decode(value: unknown): I;
  encode(value: O): unknown;
}

/** Record completion is awaited. The sink must not throw or reject and receives no input or output. */
export class Executor {
  constructor(private readonly record: (record: CallRecord) => void | Promise<void>) {}

  call<I, O, R>(action: Action<I, O, R>, context: Context<NoInfer<R>>, input: NoInfer<I>): Promise<O> {
    return this.invoke(action, context, () => action.input ? parseInput(action.input, input) : input, (output) => output);
  }

  callExternal<I, O, R>(
    action: Action<I, O, R>,
    context: Context<NoInfer<R>>,
    value: unknown,
    codec?: Codec<NoInfer<I>, NoInfer<O>>,
  ): Promise<unknown> {
    return this.invoke(
      action,
      context,
      () => {
        if (!codec) {
          if (!action.input) throw new ActionError("internal", "Action needs an external input parser");
          return parseInput(action.input, value);
        }
        try {
          const input = codec.decode(value);
          return action.input ? parseInput(action.input, input) : input;
        } catch (cause) {
          throw new ActionError("invalid_input", "Invalid input", { cause });
        }
      },
      (output) => {
        try { return codec ? codec.encode(output) : output; }
        catch (cause) { throw new ActionError("internal", "Invalid output", { cause }); }
      },
    );
  }

  private async invoke<I, O, R, T>(
    action: Action<I, O, R>,
    context: Context<R>,
    decode: () => I,
    encode: (output: O) => T,
  ): Promise<T> {
    const start = performance.now();
    let outcome: CallRecord["outcome"] = "internal";
    try {
      checkCancellation(context.signal);
      const input = decode();
      await action.validateInput?.(input);
      await action.authorize(context, input);
      checkCancellation(context.signal);
      let output: O = await action.run(context, input);
      try {
        if (action.output) output = action.output.parse(output);
        await action.validateOutput?.(output);
      } catch (cause) {
        if (cause instanceof ActionError && cause.code === "internal") throw cause;
        throw new ActionError("internal", "Invalid output", { cause });
      }
      const result = encode(output);
      outcome = "ok";
      return result;
    } catch (error) {
      const failure = actionError(error);
      outcome = failure.code;
      throw failure;
    } finally {
      await this.record({
        callId: context.callId,
        action: action.id,
        subject: context.principal?.subject,
        outcome,
        durationMs: performance.now() - start,
      });
    }
  }
}

function parseInput<I>(parser: Parser<I>, value: unknown): I {
  try { return parser.parse(value); }
  catch (cause) { throw new ActionError("invalid_input", "Invalid input", { cause }); }
}

/** Cooperative cancellation does not undo committed effects or race a running handler. */
export function checkCancellation(signal: AbortSignal): void {
  if (signal.aborted) throw new ActionError("cancelled", "Call cancelled");
}
