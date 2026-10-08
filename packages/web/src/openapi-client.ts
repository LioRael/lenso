import { ORPCError } from "@orpc/client";
import {
  createProblemDetails,
  problemType,
  type ProblemDetails,
  type ProblemDetailsOptions,
} from "./problem-details";

const MAX_ERROR_BODY_BYTES = 8 * 1024;
const SAFE_HEADERS = {
  "content-type": "application/problem+json",
  "standard-server": "json",
};

type FetchTransport = (...args: Parameters<typeof globalThis.fetch>) => Promise<Response>;

function canonicalResponse(
  policy: ReturnType<typeof createProblemDetails>,
  code: string,
  instance?: string,
): Response {
  const problem = policy.fromCode(code);
  const safe: ProblemDetails = {
    ...problem,
    ...(instance ? { instance } : {}),
  };
  return new Response(JSON.stringify(safe), { status: safe.status, headers: SAFE_HEADERS });
}

/** Guard OpenAPILink's pre-decoder JSON parsing against unsafe error bodies. */
export function createProblemDetailsFetch(
  options: Pick<ProblemDetailsOptions, "codes"> & { fetch?: FetchTransport } = {},
): FetchTransport {
  const policy = createProblemDetails({ codes: options.codes });
  const fetcher = options.fetch ?? globalThis.fetch;

  return async (input, init) => {
    let response: Response;
    try {
      response = await fetcher(input, init);
    } catch (cause) {
      throw new ORPCError("INTERNAL_SERVER_ERROR", {
        message: "Internal Server Error",
        cause,
      });
    }
    if (response.status < 400) return response;
    if (
      response.headers.get("content-type")?.split(";")[0]?.trim().toLowerCase() !==
      "application/problem+json"
    ) {
      await response.body?.cancel().catch(() => {});
      return canonicalResponse(policy, "INTERNAL_SERVER_ERROR");
    }

    const bytes: Uint8Array[] = [];
    let size = 0;
    const reader = response.body?.getReader();
    if (reader) {
      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          size += value.byteLength;
          if (size > MAX_ERROR_BODY_BYTES) {
            await reader.cancel();
            return canonicalResponse(policy, "INTERNAL_SERVER_ERROR");
          }
          bytes.push(value);
        }
      } catch {
        try {
          await reader.cancel();
        } catch {}
        return canonicalResponse(policy, "INTERNAL_SERVER_ERROR");
      } finally {
        reader.releaseLock();
      }
    }

    try {
      const body = new Uint8Array(size);
      let offset = 0;
      for (const chunk of bytes) {
        body.set(chunk, offset);
        offset += chunk.byteLength;
      }
      const value: unknown = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(body));
      if (!value || typeof value !== "object")
        return canonicalResponse(policy, "INTERNAL_SERVER_ERROR");
      const data = value as Record<string, unknown>;
      const code = typeof data.code === "string" ? data.code : "";
      const definition = Object.hasOwn(policy.definitions, code)
        ? policy.definitions[code]
        : undefined;
      if (
        !definition ||
        data.type !== problemType(code) ||
        data.status !== response.status ||
        response.status !== definition.status
      )
        return canonicalResponse(policy, "INTERNAL_SERVER_ERROR");
      const instance =
        typeof data.instance === "string" &&
        /^urn:uuid:[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
          data.instance,
        )
          ? data.instance
          : undefined;
      return canonicalResponse(policy, code, instance);
    } catch {
      return canonicalResponse(policy, "INTERNAL_SERVER_ERROR");
    }
  };
}

/** Pass this callback as OpenAPILink's customErrorResponseBodyDecoder. */
export function createProblemDetailsDecoder(options: ProblemDetailsOptions = {}) {
  const policy = createProblemDetails(options);
  return (body: unknown, response: { status: number; headers: Record<string, unknown> }) => {
    const opaque = () =>
      new ORPCError("INTERNAL_SERVER_ERROR", { message: "Internal Server Error" });
    if (
      typeof response.headers["content-type"] !== "string" ||
      response.headers["content-type"].split(";")[0]?.trim().toLowerCase() !==
        "application/problem+json" ||
      !body ||
      typeof body !== "object"
    )
      return opaque();
    const value = body as Record<string, unknown>;
    const code = typeof value.code === "string" ? value.code : "";
    const definition = Object.hasOwn(policy.definitions, code)
      ? policy.definitions[code]
      : undefined;
    if (
      !definition ||
      value.type !== problemType(code) ||
      value.status !== response.status ||
      response.status !== definition.status
    )
      return opaque();
    // Do not trust a remote detail/data field, even when its code is recognized.
    return new ORPCError(code, { message: definition.title });
  };
}
