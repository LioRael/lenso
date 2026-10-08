import { context, isSpanContextValid, trace } from "@opentelemetry/api";
import pino, { type DestinationStream, type Logger, type LoggerOptions } from "pino";
import pinoPretty from "pino-pretty";

export type { Logger, LoggerOptions } from "pino";

export interface CreateLoggerOptions<
  CustomLevels extends string = never,
> extends LoggerOptions<CustomLevels> {
  /** Use readable output in development. JSON remains the default. */
  pretty?: boolean;
  /** Add trace identifiers from the active OpenTelemetry context at each write. */
  traceContext?: boolean;
  /** Enrich an existing logger without taking ownership of its lifecycle. */
  logger?: Logger<CustomLevels>;
  /** Optional destination stream, primarily useful for embedding and tests. */
  stream?: DestinationStream;
}

function traceBindings(): Record<string, string> {
  const spanContext = trace.getSpan(context.active())?.spanContext();
  if (!spanContext || !isSpanContextValid(spanContext)) return {};
  return { traceId: spanContext.traceId, spanId: spanContext.spanId };
}

const defaultRedaction = {
  paths: [
    "authorization",
    "cookie",
    "password",
    "token",
    "secret",
    "session",
    "credential",
    "credentials",
    "signature",
    "connectionString",
    "Authorization",
    "Cookie",
    "sessionCredential",
    "sessionToken",
    "signedUrl",
    "databaseUrl",
    "body",
    "payload",
    "*.authorization",
    "*.cookie",
    "*.password",
    "*.token",
    "*.secret",
    "*.session",
    "*.credential",
    "*.credentials",
    "*.signature",
    "*.connectionString",
    "*.Authorization",
    "*.Cookie",
    "*.sessionCredential",
    "*.sessionToken",
    "*.signedUrl",
    "*.databaseUrl",
    "*.body",
    "*.payload",
    "request.headers.authorization",
    "request.headers.cookie",
    "req.headers.authorization",
    "req.headers.cookie",
  ],
  censor: "[REDACTED]",
};

function safeError(error: unknown): unknown {
  return error instanceof Error ? { type: "Error" } : error;
}

function safeArguments(args: unknown[], errorKey = "err"): unknown[] {
  const [fields, ...rest] = args;
  const message = rest[0] === undefined ? ["Error details omitted", ...rest.slice(1)] : rest;
  if (fields instanceof Error) {
    return [{ [errorKey]: fields }, ...message];
  }
  if (
    fields &&
    typeof fields === "object" &&
    (fields as Record<string, unknown>)[errorKey] instanceof Error
  ) {
    return [fields, ...message];
  }
  return args;
}

/**
 * Creates a Pino logger. The package does not register an OTel provider or
 * own/close an externally supplied logger.
 */
export function createLogger<CustomLevels extends string = never>(
  options: CreateLoggerOptions<CustomLevels> = {},
): Logger<CustomLevels> {
  const { pretty = false, traceContext = true, logger, stream, ...loggerOptions } = options;

  if (logger) {
    const child = logger.child({});
    if (!traceContext) return child;
    return new Proxy(child, {
      get(target, property) {
        if (property === "child") {
          return (bindings: Record<string, unknown>, childOptions?: pino.ChildLoggerOptions) =>
            createLogger({ logger: target.child(bindings, childOptions) });
        }
        if (typeof property === "string" && property in target.levels.values) {
          const method = Reflect.get(target, property, target) as (...args: any[]) => unknown;
          return (...args: unknown[]) => {
            const [fields, ...rest] = safeArguments(
              args,
              Reflect.get(target, pino.symbols.errorKeySym),
            );
            const traceFields = traceBindings();
            if (!fields || typeof fields !== "object") {
              return method.call(target, traceFields, fields, ...rest);
            }
            return method.call(target, { ...(fields as object), ...traceFields }, ...rest);
          };
        }
        const value = Reflect.get(target, property, target);
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
  }

  if (stream && loggerOptions.transport) {
    throw new Error("A custom stream cannot be combined with a transport.");
  }
  if (pretty && (stream || loggerOptions.transport)) {
    throw new Error("Pretty output cannot be combined with a custom stream or transport.");
  }
  const userMixin = loggerOptions.mixin;
  const destination =
    stream ??
    (pretty ? pinoPretty({ destination: 2, colorize: false, sync: true }) : process.stderr);
  return pino<CustomLevels>(
    {
      ...loggerOptions,
      redact: loggerOptions.redact ?? defaultRedaction,
      serializers: {
        err: safeError,
        error: safeError,
        [loggerOptions.errorKey ?? "err"]: safeError,
        ...loggerOptions.serializers,
      },
      hooks: {
        ...loggerOptions.hooks,
        logMethod(args, method, level) {
          const safe = safeArguments(args, loggerOptions.errorKey) as Parameters<typeof method>;
          if (loggerOptions.hooks?.logMethod) {
            loggerOptions.hooks.logMethod.call(this, safe, method, level);
          } else {
            method.apply(this, safe);
          }
        },
      },
      ...(traceContext
        ? {
            mixin: (fields, level, instance) => ({
              ...userMixin?.(fields, level, instance),
              ...traceBindings(),
            }),
            mixinMergeStrategy: (fields, mixin) => ({
              ...(loggerOptions.mixinMergeStrategy
                ? loggerOptions.mixinMergeStrategy(fields, mixin)
                : { ...mixin, ...fields }),
              ...traceBindings(),
            }),
          }
        : {}),
    },
    loggerOptions.transport ? undefined : destination,
  );
}
