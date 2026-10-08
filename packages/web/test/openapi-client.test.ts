import { expect, test } from "bun:test";
import { os, type RouterClient } from "@orpc/server";
import { createORPCClient } from "@orpc/client";
import { openapi } from "@orpc/openapi";
import { OpenAPILink } from "@orpc/openapi/fetch";
import { createProblemDetails, problemType } from "../src/problem-details";
import { createProblemDetailsDecoder, createProblemDetailsFetch } from "../src/openapi-client";

const router = {
  read: os.meta(openapi({ method: "GET", path: "/read" })).handler(() => "ok"),
};

const stubFetch = (response: () => Response) => async () => response();

function client(response: Response) {
  const fetch = createProblemDetailsFetch({ fetch: stubFetch(() => response) });
  const link = new OpenAPILink(router, {
    origin: "https://example.test",
    customErrorResponseBodyDecoder: createProblemDetailsDecoder(),
    fetch,
  });
  return createORPCClient(link) as RouterClient<typeof router>;
}

test("guards malformed, oversized, and unrecognized proxy error bodies before OpenAPILink parses them", async () => {
  for (const response of [
    new Response("SECRET_FROM_UNKNOWN_PROXY", {
      status: 500,
      headers: { "content-type": "application/problem+json", "standard-server": "json" },
    }),
    new Response("x".repeat(8193), {
      status: 500,
      headers: { "content-type": "application/problem+json" },
    }),
    new Response(JSON.stringify({ code: "UNKNOWN_SECRET", detail: "SECRET" }), {
      status: 500,
      headers: { "content-type": "application/problem+json" },
    }),
  ]) {
    await expect(client(response).read()).rejects.toMatchObject({
      code: "INTERNAL_SERVER_ERROR",
      message: "Internal Server Error",
    });
  }
});

test("canonicalizes recognized errors and preserves only a canonical occurrence UUID", async () => {
  const details = createProblemDetails().fromCode("CONFLICT");
  const makeResponse = () =>
    new Response(JSON.stringify({ ...details, detail: "secret", cause: "secret" }), {
      status: 409,
      headers: {
        "content-type": "application/problem+json",
        "x-secret": "never-forward",
        "standard-server": "json",
      },
    });

  const guarded = await createProblemDetailsFetch({
    fetch: stubFetch(makeResponse),
  })("https://example.test");
  const body = await guarded.json();
  expect(body).toMatchObject({
    code: "CONFLICT",
    type: problemType("CONFLICT"),
    instance: details.instance,
    status: 409,
  });
  expect(body).not.toHaveProperty("cause");
  expect(guarded.headers.get("x-secret")).toBeNull();
  expect(guarded.headers.get("standard-server")).toBe("json");
  await expect(client(makeResponse()).read()).rejects.toMatchObject({
    code: "CONFLICT",
    message: "Conflict",
  });
});

test("successful streaming response is returned unchanged and unread", async () => {
  let consumed = false;
  const response = new Response(
    new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("data: ok\n\n"));
      },
      pull() {
        consumed = true;
      },
    }),
    { headers: { "content-type": "text/event-stream" } },
  );
  const guarded = await createProblemDetailsFetch({ fetch: stubFetch(() => response) })(
    "https://example.test",
  );
  expect(guarded).toBe(response);
  expect(consumed).toBe(false);
});
