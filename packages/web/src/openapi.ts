import { ORPCError, walkProcedureContractsSync, type Context, type Router } from "@orpc/server";
import {
  OpenAPIGenerator,
  getOpenAPIMeta,
  type OpenAPIGeneratorOptions,
  type OpenAPIGeneratorGenerateOptions,
  type OpenAPIDocument,
} from "@orpc/openapi";
import { OpenAPIHandler } from "@orpc/openapi/fetch";
import { wrapAsyncIteratorPreservingEventMeta } from "@orpc/client";
import {
  createProblemDetails,
  problemDetailsSchema,
  type ProblemDetailsOptions,
} from "./problem-details";

export interface OpenAPIAdapterOptions<T extends Context> extends ProblemDetailsOptions {
  /** Application-owned allowlist of the same procedure objects used by RPC. Omitted exposes none. */
  selectedRouter?: Router<T>;
  prefix: `/${string}`;
  /** Required admission policy. Explicit public access: () => {}. Throw to refuse. */
  authenticate(request: Request, context: T): void | Promise<void>;
  converters?: OpenAPIGeneratorOptions["converters"];
}

export interface OpenAPIAdapter<T extends Context> {
  handle(request: Request, context: T): Promise<Response | undefined>;
  fetch(context: T & { request: Request }): Promise<Response | undefined>;
  generateSpec(
    base: OpenAPIGeneratorGenerateOptions<"3.2.0">["base"],
  ): Promise<OpenAPIDocument<"3.2.0">>;
}

function canonicalPath(value: unknown, templates: boolean): value is `/${string}` {
  if (typeof value !== "string" || !value.startsWith("/")) return false;
  if (value === "/") return true;
  const parameters = new Set<string>();
  return value
    .slice(1)
    .split("/")
    .every((segment) => {
      if (templates && /^\{\+?[A-Za-z_][A-Za-z0-9_]*\}$/.test(segment)) {
        const name = segment.replace(/^\{\+?|\}$/g, "");
        if (parameters.has(name)) return false;
        parameters.add(name);
        return true;
      }
      return segment !== "." && segment !== ".." && /^[A-Za-z0-9._~!$&'()*+,;=:@-]+$/.test(segment);
    });
}

function successStatus(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 200 && value <= 399;
}

export function createOpenAPIAdapter<T extends Context>(
  options: OpenAPIAdapterOptions<T>,
): OpenAPIAdapter<T> {
  if (!canonicalPath(options.prefix, false) || options.prefix === "/") {
    throw new TypeError(
      "OpenAPI prefix must be an explicit non-root path without a trailing slash",
    );
  }
  if (typeof options.authenticate !== "function") {
    throw new TypeError("OpenAPI authenticate callback is required (including for public access)");
  }
  let snapshot: unknown = Object.create(null);
  const lazy = walkProcedureContractsSync(options.selectedRouter ?? {}, (procedure, path) => {
    const metadata = getOpenAPIMeta(procedure);
    if (!metadata?.method || !metadata.path) {
      throw new TypeError(
        `Selected OpenAPI procedure ${path.join(".")} requires explicit method and path metadata`,
      );
    }
    if (!["HEAD", "GET", "POST", "PUT", "DELETE", "PATCH", "QUERY"].includes(metadata.method)) {
      throw new TypeError(
        `Selected OpenAPI procedure ${path.join(".")} has an unsupported HTTP method`,
      );
    }
    if (
      !canonicalPath(metadata.path, true) ||
      (metadata.prefix !== undefined && !canonicalPath(metadata.prefix, false))
    ) {
      throw new TypeError(
        `Selected OpenAPI procedure ${path.join(".")} requires canonical absolute path/prefix metadata`,
      );
    }
    if (metadata.successStatus !== undefined && !successStatus(metadata.successStatus)) {
      throw new TypeError(
        `Selected OpenAPI procedure ${path.join(".")} successStatus must be a safe integer from 200 to 399`,
      );
    }
    if (!path.length) {
      snapshot = procedure;
      return;
    }
    let container = snapshot as Record<string, unknown>;
    for (const segment of path.slice(0, -1)) {
      if (!Object.hasOwn(container, segment)) container[segment] = Object.create(null);
      container = container[segment] as Record<string, unknown>;
    }
    container[path[path.length - 1]!] = procedure;
  });
  if (lazy.length)
    throw new TypeError(
      "OpenAPI selectedRouter must be eager; select loaded procedures explicitly",
    );
  // Copy router containers, not procedures or caller-owned declarations.
  const router = snapshot as Router<T>;
  const policy = createProblemDetails(options);
  function safeStream(value: unknown): unknown {
    if (
      !value ||
      typeof value !== "object" ||
      !("next" in value) ||
      typeof value.next !== "function" ||
      !(Symbol.asyncIterator in value)
    ) {
      return value;
    }
    return wrapAsyncIteratorPreservingEventMeta(value as AsyncIterator<unknown>, {
      mapError(error) {
        const code = policy.codeFor(error);
        return new ORPCError(code, {
          message: policy.definitions[code]!.title,
          cause: error,
        });
      },
    });
  }
  const handler = new OpenAPIHandler<T>(router, {
    errorStatusMap: policy.errorStatusMap,
    clientInterceptors: [
      async ({ next, procedure }) => {
        const output = await next();
        // beta.42's SSE serializer has its own error encoding, bypassing the
        // Problem Details encoder. Sanitize at the producer boundary instead.
        if (
          getOpenAPIMeta(procedure)?.outputStructure === "detailed" &&
          output &&
          typeof output === "object"
        ) {
          if ("status" in output && output.status !== undefined && !successStatus(output.status)) {
            throw new ORPCError("INTERNAL_SERVER_ERROR");
          }
          return "body" in output ? { ...output, body: safeStream(output.body) } : output;
        }
        return safeStream(output);
      },
    ],
    interceptors: [
      async ({ next }) => {
        try {
          return await next();
        } catch (error) {
          throw new ORPCError(policy.codeFor(error), { cause: error });
        }
      },
    ],
    customErrorResponseBodyEncoder: (error) => policy.fromCode(error.code),
    routingInterceptors: [
      async ({ next }) => {
        const result = await next();
        if (result.matched && result.response.status >= 400) {
          result.response.headers["content-type"] = "application/problem+json";
          result.response.headers["standard-server"] = "json";
          // Standard Server forces JSON objects to application/json. Encode here,
          // before Fetch headers are committed, not by rewriting a live response.
          result.response.body = new Blob([JSON.stringify(result.response.body)], {
            type: "application/problem+json",
          });
        }
        return result;
      },
    ],
  });
  async function handle(request: Request, context: T): Promise<Response | undefined> {
    const pathname = new URL(request.url).pathname;
    if (pathname !== options.prefix && !pathname.startsWith(`${options.prefix}/`)) return undefined;
    try {
      await options.authenticate(request, context);
      const result = await handler.handle(request, { prefix: options.prefix, context });
      return result.matched ? result.response : policy.response(new ORPCError("NOT_FOUND"));
    } catch (error) {
      return policy.response(error);
    }
  }
  return {
    handle,
    fetch: (context: T & { request: Request }) => handle(context.request, context),
    async generateSpec(base: OpenAPIGeneratorGenerateOptions<"3.2.0">["base"]) {
      type Converter = NonNullable<OpenAPIGeneratorOptions["converters"]>[number];
      const strictConverter: Converter = {
        condition: () => true,
        convert(schema, direction) {
          if (!schema) return [{}, true];
          const converter = options.converters?.find((item) => item.condition(schema, direction));
          if (converter) return converter.convert(schema, direction);
          const standard = schema["~standard"] as (typeof schema)["~standard"] & {
            jsonSchema?: {
              input(options: { target: string }): ReturnType<Converter["convert"]>[0];
              output(options: { target: string }): ReturnType<Converter["convert"]>[0];
            };
          };
          if (!standard.jsonSchema || typeof standard.jsonSchema[direction] !== "function") {
            throw new TypeError("Selected OpenAPI schema has no supported JSON Schema converter");
          }
          // Unlike oRPC's permissive fallback, conversion exceptions are diagnostics.
          const jsonSchema = standard.jsonSchema[direction]({ target: "draft-2020-12" });
          const validation = standard.validate(undefined);
          if (validation instanceof Promise) {
            void validation.catch(() => {});
            return [jsonSchema, false];
          }
          return [
            jsonSchema,
            !validation.issues && (direction === "input" || validation.value === undefined),
          ];
        },
      };
      const spec = await new OpenAPIGenerator({ converters: [strictConverter] }).generate(router, {
        base: { ...base, servers: [{ url: options.prefix }] },
        errorStatusMap: policy.errorStatusMap,
        customErrorResponseBodySchema: () => problemDetailsSchema,
      });
      for (const path of Object.values(spec.paths ?? {})) {
        if (!path) continue;
        for (const method of [
          "get",
          "post",
          "put",
          "patch",
          "delete",
          "head",
          "options",
          "trace",
          "query",
        ] as const) {
          const operation = path[method];
          if (!operation) continue;
          // Admission and undeclared failures can occur on every selected procedure.
          for (const status of new Set(Object.values(policy.errorStatusMap))) {
            operation.responses ??= {};
            operation.responses[String(status)] = {
              description: "Problem Details",
              content: { "application/problem+json": { schema: problemDetailsSchema } },
            };
          }
        }
      }
      // Defaults and examples are value-bearing documentation, not shape.
      const maps = new Set([
        "properties",
        "patternProperties",
        "$defs",
        "definitions",
        "schemas",
        "paths",
        "responses",
        "content",
        "headers",
        "securitySchemes",
      ]);
      function omitValues(value: unknown, entries = false): void {
        if (!value || typeof value !== "object") return;
        for (const [key, child] of Object.entries(value)) {
          if (!entries && (key === "default" || key === "example" || key === "examples")) {
            delete (value as Record<string, unknown>)[key];
          } else {
            omitValues(child, !entries && maps.has(key));
          }
        }
      }
      omitValues(spec);
      return spec;
    },
  };
}
